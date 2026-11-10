//! Headless edit operations (Sprint 15, T002 / CAP-003).
//!
//! A maker applies edit operations to the open composition from T001
//! ([`SdkSession::open`](crate::SdkSession::open)): elements,
//! structure, tweens, keyframes, labels, and arc paths. Every applied
//! edit is a patch paired with its inverse ([`Patch`]); the staged
//! effect is visible through
//! [`SdkSession::effective_composition`](crate::SdkSession::effective_composition)
//! before anything persists ([`SdkSession::preview_edits`]), and
//! [`SdkSession::persist`](crate::SdkSession::persist) saves the
//! override delta so reopening replays it keeping every position.
//!
//! Persistence boundary: edits apply to the in-memory composition
//! model only. Persisting writes one SDK-owned delta file,
//! `<project>/.framesmith/sdk-overrides.json`
//! ([`sdk_overrides_path_for`]), and nothing else — the composition
//! sources on disk stay byte-identical, no render output is produced,
//! and render, check, and validation never read the delta. Reopening
//! replays the delta onto the validated composition, so an SDK-added
//! element returns at the same position.
//!
//! Refusal boundary: capability and value checks run before anything
//! changes. A refused edit names a reason code (`E_SDK_*`) with the
//! cause and the fix, and leaves the composition, the staged set, and
//! history exactly unchanged.
//!
//! Naming boundary: style attributes stay CSS-like hyphenated end to
//! end (`font-family`, `border-color`, `border-width`, `fill-rule`),
//! including inside property names and the persisted delta. An
//! unknown `font-family` list is never resolved or substituted here:
//! the authored try-order passes through unchanged and fail-closed
//! resolution stays at render time, exactly like `check`.

use std::path::{Path, PathBuf};

use notation::model::{
    Align, AudioClip, BlendMode, BoxClip, Clip, Composition, Crossfade, EaseDecl, FillPaint,
    FillRule, Fit, FontStyle, ImageClip, KeyDecl, KeyStop, MotionAsserts, NestClip, Overflow,
    PathClip, Placement, Rgb, SetDecl, SoundFx, StrokePosition, TextClip, TextDirection, TrackKind,
    Turn3D, TweenDecl, TweenKind, VideoClip, WaveformDraw, Wrap, MOTION_ALIASES, MOTION_PROPERTIES,
};
use notation::model::{FILTER_PROPERTIES, LATER_PROPERTIES, SOUND_PROPERTIES};

/// SDK-owned override delta file name inside the hidden project
/// directory: the persisted forward edit log. Render, check, and
/// validation never read it; only [`crate::SdkSession::open`] replays
/// it and [`crate::SdkSession::persist`] writes it.
pub const SDK_OVERRIDES_FILE: &str = "sdk-overrides.json";

/// Version of the persisted override delta. A file carrying another
/// version refuses the open naming the file, never guessed.
pub const SDK_OVERRIDES_VERSION: u32 = 1;

/// Hidden override delta path for one project folder.
pub fn sdk_overrides_path_for(folder: &Path) -> PathBuf {
    folder
        .join(crate::HISTORY_DIR_NAME)
        .join(SDK_OVERRIDES_FILE)
}

/// Why a headless edit refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusalKind {
    /// The session is read-only (`persist: false`): nothing may apply.
    ReadOnly,
    /// The session is a dry-run preview: edits stage, never persist.
    Preview,
    /// The referenced clip is not in the open composition.
    UnknownTarget,
    /// The operation is outside composition capability.
    Unsupported,
    /// A value breaks its rule (color, easing, number, path, order).
    BadValue,
    /// The hidden delta file cannot be written.
    Unwritable,
    /// Undo names no patch target: the undo stack is empty.
    NothingToUndo,
    /// Redo names no patch target: the redo stack is empty.
    NothingToRedo,
    /// Staged patches fail validation: persistence blocked, staged
    /// state kept (Sprint 15, T003 / REC-002).
    BrokenTurn,
    /// An exclusive history turn held by another writer blocks this
    /// persist (Sprint 15, T008 / REC-004): staged work stays
    /// preserved unapplied.
    GuardHeld,
}

/// A refused headless edit: the kind, the reason code, and a message
/// naming the cause with the fix. Nothing starts on refusal: no
/// staged change, no composition change, no history change.
#[derive(Debug, Clone)]
pub struct EditRefusal {
    pub kind: EditRefusalKind,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for EditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// Reason code: the session is read-only.
pub const E_SDK_READ_ONLY: &str = "E_SDK_READ_ONLY";
/// Reason code: a preview session never persists.
pub const E_SDK_PREVIEW: &str = "E_SDK_PREVIEW";
/// Reason code: the clip reference resolves to nothing.
pub const E_SDK_UNKNOWN_TARGET: &str = "E_SDK_UNKNOWN_TARGET";
/// Reason code: the operation is outside capability.
pub const E_SDK_UNSUPPORTED: &str = "E_SDK_UNSUPPORTED";
/// Reason code: a value breaks its rule.
pub const E_SDK_BAD_VALUE: &str = "E_SDK_BAD_VALUE";
/// Reason code: the hidden delta cannot be written.
pub const E_SDK_UNWRITABLE: &str = "E_SDK_UNWRITABLE";
/// Reason code: undo names no patch target (empty undo stack).
pub const E_SDK_NOTHING_TO_UNDO: &str = "E_SDK_NOTHING_TO_UNDO";
/// Reason code: redo names no patch target (empty redo stack).
pub const E_SDK_NOTHING_TO_REDO: &str = "E_SDK_NOTHING_TO_REDO";
/// Reason code: staged patches fail validation, so persistence is
/// blocked naming the offenders while staged state is kept.
pub const E_SDK_BROKEN_TURN: &str = "E_SDK_BROKEN_TURN";
/// Reason code: an exclusive history turn held by another writer
/// blocks this persist; staged work stays preserved unapplied.
pub const E_SDK_GUARD_HELD: &str = "E_SDK_GUARD_HELD";

/// Maximum undoable patch entries (Sprint 15, T003 / CAP-004): undo
/// and redo walk up to this many entries; beyond it the oldest entry
/// drops and the undo listing reports the drop.
pub const MAX_UNDO_ENTRIES: usize = 100;

fn refused(kind: EditRefusalKind, code: &'static str, message: String) -> EditRefusal {
    EditRefusal {
        kind,
        code,
        message,
    }
}

/// Reference to one clip in the open composition: by stable `id` when
/// set, else by document index. Resolution prefers `id`; an `id` that
/// names nothing refuses even when an index is also present, never
/// falling back silently. At least one of the two must be set.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ClipRef {
    pub id: Option<String>,
    pub index: Option<usize>,
}

impl ClipRef {
    /// Reference the clip carrying this stable `id`.
    pub fn by_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            index: None,
        }
    }

    /// Reference the clip at this document index.
    pub fn by_index(index: usize) -> Self {
        Self {
            id: None,
            index: Some(index),
        }
    }
}

impl std::fmt::Display for ClipRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.id, self.index) {
            (Some(id), Some(i)) => write!(f, "clip id {id:?} (index {i})"),
            (Some(id), None) => write!(f, "clip id {id:?}"),
            (None, Some(i)) => write!(f, "clip index {i}"),
            (None, None) => write!(f, "clip <unnamed>"),
        }
    }
}

/// One element the SDK can add (Sprint 15, T002; nest joins in Sprint
/// 18, T001; image, video, and audio join in Sprint 18, T002): text,
/// box, path, nest, image, video, and audio clips with the fields the
/// composition model needs. Field names map to hyphenated KDL style
/// attributes (`font-family`, `fill-rule`); `font-family` is one
/// comma-separated try-order list, preserved as authored. A nest names
/// its composition with `target` (a composition `name` or `id`);
/// whether the target exists, cycles, and depth is checked by the
/// caller (the Studio Compositions panel), which reads the whole
/// project — the open session alone cannot see sibling compositions.
/// A media element names its project-relative media file with `path`;
/// whether the file exists is checked by the caller (the Studio Assets
/// panel), which reads the project folder — the open session alone
/// cannot see sibling files. An `.svg` source inserts as an `image`
/// clip, which accepts SVG sources like the notation does. Flex
/// elements stay outside this slice: adding one refuses with
/// [`E_SDK_UNSUPPORTED`] by naming a supported kind instead.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum NewElement {
    Text {
        id: Option<String>,
        label: Option<String>,
        value: String,
        start: f64,
        duration: f64,
        x: f64,
        y: f64,
        size: f64,
        color: String,
        font_family: Option<String>,
    },
    Box {
        id: Option<String>,
        label: Option<String>,
        start: f64,
        duration: f64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        fill: String,
    },
    Path {
        id: Option<String>,
        label: Option<String>,
        start: f64,
        duration: f64,
        x: f64,
        y: f64,
        d: String,
        fill: String,
        fill_rule: Option<String>,
    },
    Nest {
        id: Option<String>,
        label: Option<String>,
        target: String,
        start: f64,
        duration: f64,
    },
    Image {
        id: Option<String>,
        label: Option<String>,
        path: String,
        start: f64,
        duration: f64,
    },
    Video {
        id: Option<String>,
        label: Option<String>,
        path: String,
        start: f64,
        duration: f64,
    },
    Audio {
        id: Option<String>,
        label: Option<String>,
        path: String,
        start: f64,
        duration: f64,
    },
}

impl NewElement {
    /// The stable `id` the added element would carry, when declared.
    pub fn id(&self) -> Option<&str> {
        match self {
            NewElement::Text { id, .. }
            | NewElement::Box { id, .. }
            | NewElement::Path { id, .. }
            | NewElement::Nest { id, .. }
            | NewElement::Image { id, .. }
            | NewElement::Video { id, .. }
            | NewElement::Audio { id, .. } => id.as_deref(),
        }
    }

    /// Display name of the element kind (`text element`, ...).
    pub fn what(&self) -> &'static str {
        match self {
            NewElement::Text { .. } => "text element",
            NewElement::Box { .. } => "box element",
            NewElement::Path { .. } => "path element",
            NewElement::Nest { .. } => "nest element",
            NewElement::Image { .. } => "image element",
            NewElement::Video { .. } => "video element",
            NewElement::Audio { .. } => "audio element",
        }
    }
}

/// One keyframe stop of an [`EditOp::AddKey`]: the instant with the
/// authored value and the easing of the segment leaving it (none
/// means the key easing, then linear). Values are authored text —
/// numbers for numeric properties, `#rgb`/`#rrggbb` for `fill` and
/// `border-color` — parsed per property at apply time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeyStopInput {
    pub at: f64,
    pub value: String,
    pub ease: Option<String>,
}

/// One audio-rack lane range edit (Sprint 18, T007 / CAP-007): how an
/// [`EditOp::RewriteLane`] rewrites the `volume`/`pan` key stops
/// inside its window. Every variant keeps stops outside the window
/// exactly as they were and replaces the stops inside with exact
/// points — never invented ones. Boundary values a variant omits are
/// read from the lane itself (linear interpolation between neighbor
/// stops, edge hold outside, the property's own value on a keyless
/// lane), documented per variant so the rack proposes honest points.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "edit", rename_all = "lowercase")]
pub enum LaneEdit {
    /// Straight ride across the window: one stop at each edge. `from`
    /// and `to` are authored numbers (`None` reads the lane value at
    /// that edge).
    Ramp {
        from: Option<String>,
        to: Option<String>,
    },
    /// Rise to `peak` at the window middle, then fall back to the
    /// edge values (read from the lane).
    Swell { peak: String },
    /// Fall to `trough` at the window middle, then rise back to the
    /// edge values (read from the lane).
    Dip { trough: String },
    /// Drop the interior stops the straight edge-to-edge line already
    /// predicts within `tolerance`, keeping the endpoints and every
    /// stop deviating further. A lane the tolerance already predicts
    /// refuses as already simple instead of staging a phantom patch.
    Simplify { tolerance: f64 },
}

/// One tween carried by an [`EditOp::PasteClips`]: the position of
/// its clip inside the pasted `clips` list with the validated
/// declaration. Positions remap to the landed document indices at
/// apply time, so the persisted log replays onto the same base
/// deterministically.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PastedTween {
    pub clip: usize,
    pub tween: TweenDecl,
}

/// One key carried by an [`EditOp::PasteClips`]: the position of its
/// clip inside the pasted `clips` list with the validated
/// declaration. Stop instants shift with the paste anchor at apply
/// time, so the motion stays glued to its clip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PastedKey {
    pub clip: usize,
    pub key: KeyDecl,
}

/// One set carried by an [`EditOp::PasteClips`]: the position of its
/// clip inside the pasted `clips` list with the validated
/// declaration. The hold window shifts with the paste anchor at
/// apply time, so the hold stays glued to its clip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PastedSet {
    pub clip: usize,
    pub set: SetDecl,
}

/// One headless edit operation (Sprint 15, T002): elements, structure,
/// tweens, keyframes, labels, and arc paths over the open
/// composition. Every variant persists as plain data (numbers,
/// strings, booleans), so the forward log replays on reopen; inverses
/// ([`RevertOp`]) may additionally carry model snapshots and stay in
/// memory only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum EditOp {
    /// Insert an element at `position` (append when absent).
    AddElement {
        element: NewElement,
        position: Option<usize>,
    },
    /// Remove the referenced clip.
    RemoveElement { target: ClipRef },
    /// Move the referenced clip to document index `to`.
    MoveElement { target: ClipRef, to: usize },
    /// Append a tween of one property on one clip. `kind` is
    /// `to`/`from`/`from-to`; `from`/`to` are authored text (numbers,
    /// or colors for `fill`/`border-color`); `ease` names the curve.
    AddTween {
        target: ClipRef,
        property: String,
        kind: String,
        from: Option<String>,
        to: Option<String>,
        start: f64,
        duration: f64,
        ease: String,
    },
    /// Remove the tween at this motion-directive index.
    RemoveTween { index: usize },
    /// Append a key of one property on one clip through at least two
    /// stops in rising time order.
    AddKey {
        target: ClipRef,
        property: String,
        stops: Vec<KeyStopInput>,
        ease: Option<String>,
    },
    /// Remove the key at this motion-directive index.
    RemoveKey { index: usize },
    /// Hold one value over a window (Sprint 17, T008): a `set`
    /// directive with one number and no easing. Colors hold no single
    /// number, so a set on `fill` or `border-color` refuses naming the
    /// `tween` or `key` that interpolates them instead.
    AddSet {
        target: ClipRef,
        property: String,
        value: String,
        start: f64,
        duration: f64,
    },
    /// Remove the set at this motion-directive index.
    RemoveSet { index: usize },
    /// Travel a path (Sprint 17, T008): a path tween over a window
    /// with easing. `path_to` morphs toward a second path of the same
    /// structure and `orient` rotates the clip to the path tangent. An
    /// unparseable path refuses naming its cause, and a morph over two
    /// structures refuses naming the first difference.
    AddPathTween {
        target: ClipRef,
        path: String,
        path_to: Option<String>,
        orient: bool,
        start: f64,
        duration: f64,
        ease: String,
    },
    /// Set the easing of the tween at this index (Sprint 17, T008):
    /// the bezier editor writes here. `ease` names the same curves
    /// `check` accepts; anything else refuses naming what was written.
    SetTweenEase { index: usize, ease: String },
    /// Replace the travel path of the tween at this index (Sprint 17,
    /// T008): edited motion-path points commit here. The tween must
    /// carry a path; `path_to` replaces the morph destination (`None`
    /// clears it).
    SetTweenPath {
        index: usize,
        path: String,
        path_to: Option<String>,
    },
    /// Set the editor note on one clip (`None` clears it). Notes ride
    /// along with no render effect.
    SetLabel {
        target: ClipRef,
        label: Option<String>,
    },
    /// Append one arc segment (`A` command) to the `d` of a path clip,
    /// ending at (`x`, `y`).
    AppendArc {
        target: ClipRef,
        rx: f64,
        ry: f64,
        rot: f64,
        large: bool,
        sweep: bool,
        x: f64,
        y: f64,
    },
    /// Retime one clip (Sprint 15, T007): overwrite any subset of the
    /// film window (`start`, `duration`) and — on footage-style clips
    /// (`video`, `audio`, `nest`) — the file window origin
    /// (`trim_start`). `None` keeps the staged value; a `trim_start`
    /// on any other kind refuses, never guessed. Moving in time keeps
    /// the file origin (same frames at a new film time); trimming the
    /// head passes an advanced origin so the same tail shows.
    Retiming {
        target: ClipRef,
        start: Option<f64>,
        duration: Option<f64>,
        trim_start: Option<f64>,
    },
    /// Split one clip at the film time `at` (Sprint 15, T007): the
    /// staged clip becomes two clips sharing the window at `at`, with
    /// the footage file window divided so both halves show the same
    /// frames they showed before. The first half keeps the `id`; the
    /// second half drops it so `id` values stay unique. `flex`
    /// containers refuse: splitting a laid-out subtree is outside this
    /// slice, so split its items instead.
    SplitElement { target: ClipRef, at: f64 },
    /// Duplicate one clip (Sprint 15, T007): an exact clone inserted
    /// right after the original at film time `at` (the original end
    /// when omitted). The copy drops the `id` unless `new_id` names a
    /// fresh unique one, so `id` values stay unique.
    DuplicateElement {
        target: ClipRef,
        at: Option<f64>,
        new_id: Option<String>,
    },
    /// Set the visibility switch on one clip (Sprint 15, T007): `true`
    /// skips picture and sound as if absent while the `id` stays
    /// referenceable, exactly like the validated `hidden` key.
    SetHidden { target: ClipRef, hidden: bool },
    /// Collect clips into one Layers group (Sprint 18, T004 / CAP-004):
    /// every named clip carries the group note `group:<name>` in its
    /// editor `label` slot, which rides along with no render effect —
    /// so grouping re-renders byte-identically and the Layers tree,
    /// canvas, and timeline keep selecting the same keys. Members must
    /// be ungrouped and note-free; grouping refuses otherwise naming
    /// the member and the fix. One op is one undo unit.
    GroupElements { targets: Vec<ClipRef>, name: String },
    /// Release one Layers group (Sprint 18, T004 / CAP-004): every clip
    /// still carrying the group note `group:<name>` drops it, restoring
    /// the note-free members. A name with no grouped members refuses
    /// naming the holdings; members whose note changed since grouping
    /// left the group and stay untouched. One op is one undo unit.
    UngroupElements { name: String },
    /// Set one inspector row on the referenced clip (Sprint 17,
    /// T007): `property` is a CSS-like hyphenated row key
    /// (`font-size`, `background-color`, `object-fit`, `z-index`,
    /// ...), `value` its authored text. Validation mirrors `check`:
    /// bad values and wrong-kind rows refuse with rule and location
    /// keeping the prior value, while render-deferred rows
    /// (`filter`, `transform-3d`, ...) refuse naming their
    /// milestone. Reset writes the row default through this same op.
    SetRow {
        target: ClipRef,
        property: String,
        value: String,
    },
    /// Retime one keyframe stop (Sprint 16, T007): move stop `stop` of
    /// key `key` to the composition instant `at`. `at` must be finite
    /// at or after 0 and keep the stops in strictly rising time order;
    /// anything else refuses naming the bound or the neighbor. Studio
    /// drags clamp into the target clip's film window first (showing
    /// the clamped value) and then apply here; headless callers pass
    /// the exact instant.
    RetimeKeyStop { key: usize, stop: usize, at: f64 },
    /// Edit the easing of one key (Sprint 16, T007): the key-level
    /// fallback when `stop` is `None`, else the easing of the segment
    /// leaving stop `stop`. `ease` names the same curves `check`
    /// accepts; anything else refuses naming what was written.
    SetKeyEase {
        key: usize,
        stop: Option<usize>,
        ease: String,
    },
    /// Delete one keyframe stop (Sprint 16, T007): remove stop `stop`
    /// of key `key`. A key reduced below two stops cannot stand, so
    /// deleting from a two-stop key removes the whole key instead and
    /// the receipt says so.
    RemoveKeyStop { key: usize, stop: usize },
    /// Copy one key (Sprint 16, T007): append an exact duplicate of
    /// key `key` (same target, property, stops, and easing). The copy
    /// is a valid key exactly when the original is, so copying never
    /// invents motion.
    DuplicateKey { key: usize },
    /// Rewrite one automation lane across a window (Sprint 18, T007
    /// / CAP-007): the `volume`/`pan` key stops of the sound-bearing
    /// target (`audio` and `video` clips only) inside
    /// `[start, end]` are replaced per `edit`, while stops outside
    /// the window keep their exact instant, value, and easing. The
    /// rewritten points land in one fresh key appended after the
    /// trimmed keys, so the fresh key wins overlaps in document
    /// order exactly like the render evaluates. A window holding no
    /// stops refuses naming the empty range instead of inventing
    /// points; a silent (non-sound-bearing) target refuses naming
    /// its kind.
    RewriteLane {
        target: ClipRef,
        property: String,
        start: f64,
        end: f64,
        edit: LaneEdit,
    },
    /// Paste lane stops onto one automation lane (Sprint 18, T007 /
    /// CAP-007): the authored stops (at least two, in strictly rising
    /// time order, values in the property range) land as one fresh
    /// key on the sound-bearing target's `volume`/`pan` lane, while
    /// the stops the pasted span covers are trimmed from the existing
    /// keys exactly like [`EditOp::RewriteLane`]. A rack clipboard
    /// copies stops with [`copy_lane_range`](crate::edits::copy_lane_range)
    /// and pastes them back here unchanged, so the round trip rewrites
    /// points exactly.
    PasteLane {
        target: ClipRef,
        property: String,
        stops: Vec<KeyStopInput>,
    },
    /// Paste full clipboard clips at the film instant `at` (Sprint
    /// 18, T008 / CAP-008): the earliest clip opens exactly at `at`
    /// while every other clip keeps its offset from it, tracks stay
    /// as copied, and every `id` renames (`<id>-paste`, then
    /// `<id>-paste-2`, ...) so pasted ids stay unique even within
    /// one composition. Relative `start-after` links clear — the
    /// paste lands at absolute instants — while styles, grades,
    /// effects, spans, splits, and labels ride along untouched and
    /// `font-family` try-order lists pass through fail-closed like
    /// every other write path. Carried tweens, keys, and sets remap
    /// onto the landed clips with their windows shifted by the same
    /// anchor delta, so motion stays glued to its clip. A paste
    /// naming no clips, a non-finite or negative anchor, a corrupt
    /// clip window, or motion pointing outside the pasted clips
    /// refuses before anything stages. Seating caps stay with the
    /// caller (the Studio clipboard panel probes the effective
    /// timeline first), exactly like composition placement. One op is
    /// one undo unit. `composition` scopes the paste to one timeline
    /// by name: delta replay skips the paste while another
    /// composition is open, so cross-composition pastes never leak
    /// clips into sibling timelines.
    PasteClips {
        composition: String,
        clips: Vec<Clip>,
        tweens: Vec<PastedTween>,
        keys: Vec<PastedKey>,
        sets: Vec<PastedSet>,
        at: f64,
    },
    /// Remove several clips at once (Sprint 18, T008 / CAP-008): the
    /// Studio clipboard cut resolves every target first, then removes
    /// deepest-first in one undo unit. Like [`EditOp::RemoveElement`]
    /// the clips drop with their motion references untouched, and the
    /// inverse restores the exact clip list; unlike it,
    /// `composition` scopes the removal to one timeline by name, so
    /// delta replay skips the removal while another composition is
    /// open. An unknown target refuses before anything removes.
    RemoveElements {
        composition: String,
        targets: Vec<ClipRef>,
    },
}

impl EditOp {
    /// One-line human summary, used in previews and persisted-turn
    /// descriptions.
    pub fn summary(&self) -> String {
        match self {
            EditOp::AddElement { element, position } => match position {
                Some(p) => format!("add {} at index {p}", element.what()),
                None => format!("add {} at end", element.what()),
            },
            EditOp::RemoveElement { target } => format!("remove {target}"),
            EditOp::MoveElement { target, to } => format!("move {target} to index {to}"),
            EditOp::AddTween {
                target,
                property,
                kind,
                ..
            } => format!("tween {property} ({kind}) on {target}"),
            EditOp::RemoveTween { index } => format!("remove tween {index}"),
            EditOp::AddKey {
                target, property, ..
            } => format!("key {property} on {target}"),
            EditOp::RemoveKey { index } => format!("remove key {index}"),
            EditOp::AddSet {
                target, property, ..
            } => format!("set {property} on {target}"),
            EditOp::RemoveSet { index } => format!("remove set {index}"),
            EditOp::AddPathTween { target, .. } => {
                format!("path tween on {target}")
            }
            EditOp::SetTweenPath { index, .. } => {
                format!("retarget path tween {index}")
            }
            EditOp::SetTweenEase { index, ease } => {
                format!("set ease of tween {index} to {ease:?}")
            }
            EditOp::SetLabel { target, label } => match label {
                Some(l) => format!("label {target} as {l:?}"),
                None => format!("clear label on {target}"),
            },
            EditOp::AppendArc { target, x, y, .. } => {
                format!("append arc to {target} ending at ({x},{y})")
            }
            EditOp::Retiming {
                target,
                start,
                duration,
                trim_start,
            } => {
                let mut parts = Vec::new();
                if let Some(s) = start {
                    parts.push(format!("start {s}"));
                }
                if let Some(d) = duration {
                    parts.push(format!("duration {d}"));
                }
                if let Some(t) = trim_start {
                    parts.push(format!("trim-start {t}"));
                }
                if parts.is_empty() {
                    format!("retime {target} (no change)")
                } else {
                    format!("retime {target} to {}", parts.join(" "))
                }
            }
            EditOp::SplitElement { target, at } => format!("split {target} at {at}"),
            EditOp::DuplicateElement { target, at, new_id } => match (at, new_id) {
                (Some(t), Some(n)) => format!("duplicate {target} at {t} as {n:?}"),
                (Some(t), None) => format!("duplicate {target} at {t}"),
                (None, Some(n)) => format!("duplicate {target} at end as {n:?}"),
                (None, None) => format!("duplicate {target} at end"),
            },
            EditOp::SetHidden { target, hidden } => {
                if *hidden {
                    format!("hide {target}")
                } else {
                    format!("show {target}")
                }
            }
            EditOp::GroupElements { targets, name } => {
                let keys = targets
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("group {keys} as {name:?}")
            }
            EditOp::UngroupElements { name } => format!("ungroup {name:?}"),
            EditOp::SetRow {
                target,
                property,
                value,
            } => format!("set row `{property}` to {value:?} on {target}"),
            EditOp::RetimeKeyStop { key, stop, at } => {
                format!("retime key {key} stop {stop} to {at}")
            }
            EditOp::SetKeyEase { key, stop, ease } => match stop {
                Some(s) => format!("set ease of key {key} stop {s} to {ease:?}"),
                None => format!("set ease of key {key} to {ease:?}"),
            },
            EditOp::RemoveKeyStop { key, stop } => {
                format!("remove key {key} stop {stop}")
            }
            EditOp::DuplicateKey { key } => format!("copy key {key}"),
            EditOp::RewriteLane {
                target,
                property,
                start,
                end,
                edit,
            } => {
                let what = match edit {
                    LaneEdit::Ramp { .. } => "ramp",
                    LaneEdit::Swell { .. } => "swell",
                    LaneEdit::Dip { .. } => "dip",
                    LaneEdit::Simplify { .. } => "simplify",
                };
                format!("{what} lane `{property}` on {target} across {start}..{end}")
            }
            EditOp::PasteLane {
                target,
                property,
                stops,
            } => {
                format!(
                    "paste {} lane `{property}` stop(s) on {target}",
                    stops.len()
                )
            }
            EditOp::PasteClips { clips, at, .. } => {
                format!("paste {} clip(s) at {at}", clips.len())
            }
            EditOp::RemoveElements { targets, .. } => {
                format!("remove {} clip(s)", targets.len())
            }
        }
    }
}

/// The inverse of one applied edit (Sprint 15, T002): restoring the
/// exact prior state, never an approximation. Inverses stay in memory
/// — only forward [`EditOp`] values persist — and undo/redo walking
/// (T003) consumes them. Snapshot payloads box so the small variants
/// stay small.
#[derive(Debug, Clone)]
pub enum RevertOp {
    /// Remove the clip at this index (inverts an add).
    RemoveClipAt { index: usize },
    /// Reinsert a removed clip snapshot (inverts a remove).
    InsertClip { index: usize, clip: Box<Clip> },
    /// Move a clip back (inverts a move).
    MoveClip { target: ClipRef, to: usize },
    /// Remove the tween at this index (inverts an add).
    RemoveTween { index: usize },
    /// Reinsert a removed tween snapshot (inverts a remove).
    InsertTween { index: usize, tween: Box<TweenDecl> },
    /// Remove the key at this index (inverts an add).
    RemoveKey { index: usize },
    /// Reinsert a removed key snapshot (inverts a remove).
    InsertKey { index: usize, key: Box<KeyDecl> },
    /// Restore the prior editor note (inverts a label set).
    RestoreLabel {
        target: ClipRef,
        label: Option<String>,
    },
    /// Restore the prior editor notes of one Layers group (inverts a
    /// group collect or release): each member's exact prior label,
    /// `None` where the slot was note-free.
    RestoreGroupLabels { members: Vec<GroupLabelRestore> },
    /// Restore the prior path data (inverts an arc append).
    RestorePathD { target: ClipRef, d: String },
    /// Restore the prior film and file window (inverts a retiming):
    /// the exact prior `start`, `duration`, and — on footage-style
    /// clips — `trim_start`, never an approximation.
    RestoreTiming {
        target: ClipRef,
        start: f64,
        duration: f64,
        trim_start: Option<f64>,
    },
    /// Restore the one clip a split divided (inverts a split): removes
    /// both halves and reinserts the exact snapshot at its index.
    RestoreSplit { index: usize, clip: Box<Clip> },
    /// Restore the prior visibility switch (inverts a hide/show).
    RestoreHidden { target: ClipRef, hidden: bool },
    /// Restore the prior rendered row value (inverts a row set): the
    /// exact prior text, re-applied through the same row validation
    /// so undo restores the prior value, never an approximation.
    RestoreRow {
        target: ClipRef,
        property: String,
        prior: String,
    },
    /// Restore the prior instant of one keyframe stop (inverts a
    /// retime): the exact prior `at`, never an approximation.
    RestoreKeyStopAt { key: usize, stop: usize, at: f64 },
    /// Restore the prior easing (inverts an ease edit): the key-level
    /// fallback when `stop` is `None`, else the easing of the segment
    /// leaving stop `stop`. `None` means the slot carried no easing
    /// (the key easing, then linear); `Some` carries the exact prior
    /// curve as its canonical name.
    RestoreKeyEase {
        key: usize,
        stop: Option<usize>,
        ease: Option<String>,
    },
    /// Reinsert a removed keyframe stop snapshot (inverts a stop
    /// removal): the exact prior stop at its index.
    InsertKeyStop {
        key: usize,
        stop: usize,
        at: f64,
        value: f64,
        value_color: Option<Rgb>,
        ease: Option<String>,
    },
    /// Remove the set at this index (inverts an add).
    RemoveSet { index: usize },
    /// Reinsert a removed set snapshot (inverts a remove).
    InsertSet { index: usize, set: Box<SetDecl> },
    /// Restore the prior tween easing (inverts an ease set): the exact
    /// prior curve as its canonical name.
    RestoreTweenEase { index: usize, ease: String },
    /// Restore the prior travel path (inverts a retarget): the exact
    /// prior `path` with its morph destination (`None` means the tween
    /// carried no destination).
    RestoreTweenPath {
        index: usize,
        path: Option<String>,
        path_to: Option<String>,
    },
    /// Restore the whole key list (inverts a lane rewrite or paste):
    /// the exact prior `keys` vector, never an approximation. Lane
    /// rewrites trim several keys and append one fresh key, so only
    /// the whole list restores every stop exactly; undo and redo walk
    /// patches newest-first, so no interleaved patch can sit between
    /// the rewrite and its reversal.
    RestoreKeys { keys: Vec<KeyDecl> },
    /// Restore the whole clip and motion lists (inverts a clipboard
    /// paste): the exact prior `clips`, `tweens`, `keys`, and `sets`
    /// vectors, never an approximation. A paste appends several clips
    /// plus remapped motion, so only the whole lists restore every
    /// id, window, and stop exactly; undo and redo walk patches
    /// newest-first, so no interleaved patch can sit between the
    /// paste and its reversal.
    RestorePasted {
        clips: Vec<Clip>,
        tweens: Vec<TweenDecl>,
        keys: Vec<KeyDecl>,
        sets: Vec<SetDecl>,
    },
    /// Restore the whole clip list (inverts a clipboard cut): the
    /// exact prior `clips` vector, never an approximation. A cut
    /// removes several clips deepest-first, so only the whole list
    /// restores every id and window exactly; undo and redo walk
    /// patches newest-first, so no interleaved patch can sit between
    /// the cut and its reversal.
    RestoreClips { clips: Vec<Clip> },
}

impl RevertOp {
    /// One-line human summary of what reverting restores.
    pub fn summary(&self) -> String {
        match self {
            RevertOp::RemoveClipAt { index } => format!("remove clip at index {index}"),
            RevertOp::InsertClip { index, clip } => {
                format!("reinsert {} at index {index}", clip.what())
            }
            RevertOp::MoveClip { target, to } => format!("move {target} back to index {to}"),
            RevertOp::RemoveTween { index } => format!("remove tween {index}"),
            RevertOp::InsertTween { index, .. } => format!("reinsert tween at index {index}"),
            RevertOp::RemoveKey { index } => format!("remove key {index}"),
            RevertOp::InsertKey { index, .. } => format!("reinsert key at index {index}"),
            RevertOp::RestoreLabel { target, .. } => format!("restore label on {target}"),
            RevertOp::RestoreGroupLabels { members } => {
                format!("restore group notes on {} clip(s)", members.len())
            }
            RevertOp::RestorePathD { target, .. } => format!("restore path data on {target}"),
            RevertOp::RestoreTiming { target, .. } => format!("restore timing on {target}"),
            RevertOp::RestoreSplit { index, .. } => {
                format!("restore split clip at index {index}")
            }
            RevertOp::RestoreHidden { target, .. } => format!("restore visibility on {target}"),
            RevertOp::RestoreRow {
                target, property, ..
            } => format!("restore row `{property}` on {target}"),
            RevertOp::RestoreKeyStopAt { key, stop, .. } => {
                format!("restore key {key} stop {stop} instant")
            }
            RevertOp::RestoreKeyEase { key, stop, .. } => match stop {
                Some(s) => format!("restore ease of key {key} stop {s}"),
                None => format!("restore ease of key {key}"),
            },
            RevertOp::InsertKeyStop { key, stop, .. } => {
                format!("reinsert key {key} stop at index {stop}")
            }
            RevertOp::RemoveSet { index } => format!("remove set {index}"),
            RevertOp::InsertSet { index, .. } => format!("reinsert set at index {index}"),
            RevertOp::RestoreTweenPath { index, .. } => {
                format!("restore path of tween {index}")
            }
            RevertOp::RestoreTweenEase { index, .. } => {
                format!("restore ease of tween {index}")
            }
            RevertOp::RestoreKeys { keys } => {
                format!("restore {} lane key(s)", keys.len())
            }
            RevertOp::RestorePasted { clips, .. } => {
                format!("restore pre-paste timeline ({} clip(s))", clips.len())
            }
            RevertOp::RestoreClips { clips } => {
                format!("restore {} removed clip(s)", clips.len())
            }
        }
    }
}

/// The timeline one [`EditOp`] scopes to, when it scopes to one
/// (Sprint 18, T008): clipboard pastes and cuts name their timeline,
/// so delta replay applies them only while that timeline is open.
/// `None` keeps the legacy whole-log replay every other op uses.
pub fn op_composition_scope(op: &EditOp) -> Option<&str> {
    match op {
        EditOp::PasteClips { composition, .. } => Some(composition.as_str()),
        EditOp::RemoveElements { composition, .. } => Some(composition.as_str()),
        _ => None,
    }
}

/// One applied edit: the sequence number, the forward operation, its
/// inverse, and whether it has persisted to the override delta.
#[derive(Debug, Clone)]
pub struct Patch {
    /// Sequence number in apply order (replays keep file order).
    pub seq: u64,
    /// The forward operation that applied.
    pub op: EditOp,
    /// The inverse restoring the exact prior state.
    pub inverse: RevertOp,
    /// True once the forward operation reached the delta file.
    pub persisted: bool,
}

/// The owned receipt of one [`crate::SdkSession::apply`]: the sequence
/// number with the forward operation and its inverse, so the caller
/// reads back the patch and what would undo it.
#[derive(Debug, Clone)]
pub struct AppliedEdit {
    /// Sequence number in apply order.
    pub seq: u64,
    /// The forward operation that applied.
    pub op: EditOp,
    /// The inverse restoring the exact prior state.
    pub inverse: RevertOp,
}

/// What [`crate::SdkSession::preview_edits`] reports: the staged (not
/// yet persisted) edits with the effect already visible in
/// [`crate::SdkSession::effective_composition`], and where persisting
/// would write — without writing anything.
#[derive(Debug, Clone)]
pub struct EditPreview {
    /// Staged edits not yet in the delta file.
    pub staged: usize,
    /// Edits already in the delta file (persisted or replayed).
    pub persisted: usize,
    /// Delta path a persist would write, when the session may persist
    /// and something is staged; otherwise `None` and persist writes
    /// nothing new.
    pub would_write: Option<PathBuf>,
    /// One-line summary per staged edit, in apply order.
    pub summary: Vec<String>,
}

/// What [`crate::SdkSession::persist`] wrote.
#[derive(Debug, Clone)]
pub struct PersistReport {
    /// Newly staged edits written by this persist (0 when the stage
    /// held nothing new: an undo-shrunk log still rewrites the delta,
    /// while a clean stage writes nothing).
    pub applied: usize,
    /// Delta path written, when the session rewrote the delta;
    /// otherwise `None` and nothing was written.
    pub path: Option<PathBuf>,
}

/// One undoable patch entry in an undo listing (Sprint 15, T003): the
/// sequence number with its one-line summary and whether it has
/// reached the override delta.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    /// Sequence number in apply order.
    pub seq: u64,
    /// One-line human summary of the forward operation.
    pub summary: String,
    /// True once the forward operation reached the delta file.
    pub persisted: bool,
}

/// What [`crate::SdkSession::undo_listing`] reports (Sprint 15,
/// T003 / CAP-004): the undoable depth with the redo depth, the
/// 100-entry bound, how many oldest entries dropped beyond it, and
/// the report naming the drop.
#[derive(Debug, Clone)]
pub struct UndoListing {
    /// Undoable patch entries (newest undoes first).
    pub depth: usize,
    /// Redoable patch entries (newest re-applies first).
    pub redo_depth: usize,
    /// The undo bound ([`MAX_UNDO_ENTRIES`]).
    pub capacity: usize,
    /// Oldest entries dropped beyond the bound, in total.
    pub dropped: u64,
    /// Names the drop when `dropped` is non-zero; otherwise `None`.
    pub drop_report: Option<String>,
    /// Undoable entries in apply order (oldest first).
    pub entries: Vec<UndoEntry>,
}

/// The owned receipt of one [`crate::SdkSession::undo`]: the reversed
/// patch with what would re-apply it, so the caller reads back the
/// reversal and the redo it enables.
#[derive(Debug, Clone)]
pub struct UndoReceipt {
    /// Sequence number of the reversed patch.
    pub seq: u64,
    /// One-line human summary of the reversed forward operation.
    pub summary: String,
    /// One-line human summary of the applied inverse.
    pub inverse_summary: String,
    /// True when a redo is available after this undo.
    pub redo_available: bool,
}

/// The owned receipt of one [`crate::SdkSession::redo`]: the
/// re-applied patch, symmetric to [`UndoReceipt`].
#[derive(Debug, Clone)]
pub struct RedoReceipt {
    /// Sequence number of the re-applied patch.
    pub seq: u64,
    /// One-line human summary of the re-applied forward operation.
    pub summary: String,
    /// True when further undoes remain available after this redo.
    pub undo_available: bool,
}

/// One staged patch blocking persistence (Sprint 15, T003 /
/// REC-002): the sequence number with its summary and the validation
/// reason, so the refusal names every offender.
#[derive(Debug, Clone)]
pub struct PatchOffender {
    /// Sequence number of the offending patch.
    pub seq: u64,
    /// One-line human summary of the offending forward operation.
    pub summary: String,
    /// Why the patch fails validation (carries its reason code).
    pub reason: String,
}

/// The persisted override delta: versioned forward operations only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct OverridesFile {
    pub(crate) version: u32,
    pub(crate) ops: Vec<EditOp>,
}

/// Read the persisted delta file: missing means no staged SDK edits
/// (empty, not an error). A corrupt file or another version refuses
/// the open naming the file, never guessed past.
pub(crate) fn read_overrides_file(folder: &Path) -> Result<Vec<EditOp>, crate::OpenRefusal> {
    let path = sdk_overrides_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(crate::OpenRefusal {
                kind: crate::RefusalKind::Invalid,
                message: format!(
                    "sdk refused: the SDK override delta {} cannot be read ({e}); remove or fix the file and re-open. No composition was opened.",
                    path.display()
                ),
                diagnostics: Vec::new(),
            });
        }
    };
    let file: OverridesFile = serde_json::from_str(&raw).map_err(|e| crate::OpenRefusal {
        kind: crate::RefusalKind::Invalid,
        message: format!(
            "sdk refused: the SDK override delta {} is corrupt ({e}); remove or fix the file and re-open. No composition was opened.",
            path.display()
        ),
        diagnostics: Vec::new(),
    })?;
    if file.version != SDK_OVERRIDES_VERSION {
        return Err(crate::OpenRefusal {
            kind: crate::RefusalKind::Invalid,
            message: format!(
                "sdk refused: the SDK override delta {} carries version {}, this SDK replays version {}; remove or migrate the file and re-open. No composition was opened.",
                path.display(),
                file.version,
                SDK_OVERRIDES_VERSION
            ),
            diagnostics: Vec::new(),
        });
    }
    Ok(file.ops)
}

/// Write the forward log as the persisted delta (whole-log rewrite, so
/// replays stay ordered and idempotent).
pub(crate) fn write_overrides_file(folder: &Path, ops: &[EditOp]) -> Result<(), String> {
    let path = sdk_overrides_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "the hidden SDK directory {} cannot be prepared: {e}",
                parent.display()
            )
        })?;
    }
    let file = OverridesFile {
        version: SDK_OVERRIDES_VERSION,
        ops: ops.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("the override delta cannot be encoded: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| {
        format!(
            "the SDK override delta {} cannot be written: {e}",
            path.display()
        )
    })
}

/// Parse one easing name exactly like `check` (T003 plus the Sprint 7
/// motion set): the nineteen named curves plus `bounce`, `elastic`,
/// and `hold` take no parameters; `out-back`, `cubic-bezier`,
/// `steps`, `spring`, and `wiggle` take the same parameters with the
/// same bounds. Underscores read as hyphens. Anything else is a bad
/// value naming what was written.
pub(crate) fn parse_ease(s: &str) -> Result<EaseDecl, String> {
    let t = s.trim();
    let (name, params): (String, Option<String>) = match t.find('(') {
        None => (t.replace('_', "-"), None),
        Some(i) => {
            let name = t[..i].trim().replace('_', "-");
            let inner = t[i + 1..]
                .trim()
                .strip_suffix(')')
                .ok_or_else(|| format!("easing {t:?} has unbalanced parentheses"))?;
            (name, Some(inner.to_string()))
        }
    };
    macro_rules! bare {
        ($decl:expr, $canon:expr) => {
            match params {
                None => Ok($decl),
                Some(_) => Err(format!(
                    "easing {t:?} takes no parameters; write `ease={:?}`",
                    $canon
                )),
            }
        };
    }
    match name.as_str() {
        "linear" => bare!(EaseDecl::Linear, "linear"),
        "in-quad" => bare!(EaseDecl::InQuad, "in-quad"),
        "out-quad" => bare!(EaseDecl::OutQuad, "out-quad"),
        "in-out-quad" => bare!(EaseDecl::InOutQuad, "in-out-quad"),
        "in-cubic" => bare!(EaseDecl::InCubic, "in-cubic"),
        "out-cubic" => bare!(EaseDecl::OutCubic, "out-cubic"),
        "in-out-cubic" => bare!(EaseDecl::InOutCubic, "in-out-cubic"),
        "in-quart" => bare!(EaseDecl::InQuart, "in-quart"),
        "out-quart" => bare!(EaseDecl::OutQuart, "out-quart"),
        "in-out-quart" => bare!(EaseDecl::InOutQuart, "in-out-quart"),
        "in-quint" => bare!(EaseDecl::InQuint, "in-quint"),
        "out-quint" => bare!(EaseDecl::OutQuint, "out-quint"),
        "in-out-quint" => bare!(EaseDecl::InOutQuint, "in-out-quint"),
        "in-sine" => bare!(EaseDecl::InSine, "in-sine"),
        "out-sine" => bare!(EaseDecl::OutSine, "out-sine"),
        "in-out-sine" => bare!(EaseDecl::InOutSine, "in-out-sine"),
        "in-expo" => bare!(EaseDecl::InExpo, "in-expo"),
        "out-expo" => bare!(EaseDecl::OutExpo, "out-expo"),
        "in-out-expo" => bare!(EaseDecl::InOutExpo, "in-out-expo"),
        "bounce" => bare!(EaseDecl::Bounce, "bounce"),
        "elastic" => bare!(EaseDecl::Elastic, "elastic"),
        "hold" => bare!(EaseDecl::Hold, "hold"),
        "out-back" => match params {
            Some(inner) => match ease_number(&inner) {
                Some(n) if n >= 0.0 => Ok(EaseDecl::OutBack(n)),
                _ => Err(format!(
                    "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
                )),
            },
            None => Err(format!(
                "easing {t:?} needs a numeric overshoot, e.g. `ease=\"out-back(1.7)\"`"
            )),
        },
        "cubic-bezier" => match params {
            Some(inner) => {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() != 4 {
                    return Err(format!(
                        "easing {t:?} needs four numbers, e.g. `ease=\"cubic-bezier(0.25,0.1,0.25,1)\"`"
                    ));
                }
                match (
                    ease_number(parts[0]),
                    ease_number(parts[1]),
                    ease_number(parts[2]),
                    ease_number(parts[3]),
                ) {
                    (Some(x1), Some(y1), Some(x2), Some(y2))
                        if (0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2) =>
                    {
                        Ok(EaseDecl::CubicBezier(x1, y1, x2, y2))
                    }
                    _ => Err(format!(
                        "easing {t:?} needs four numbers `cubic-bezier(x1,y1,x2,y2)` with x1 and x2 from 0 to 1"
                    )),
                }
            }
            None => Err(format!(
                "easing {t:?} needs four numbers, e.g. `ease=\"cubic-bezier(0.25,0.1,0.25,1)\"`"
            )),
        },
        "steps" => match params {
            Some(inner) => match ease_count(&inner) {
                Some(n) => Ok(EaseDecl::Steps(n)),
                _ => Err(format!(
                    "easing {t:?} needs a positive whole step count, e.g. `ease=\"steps(4)\"`"
                )),
            },
            None => Err(format!(
                "easing {t:?} needs a positive whole step count, e.g. `ease=\"steps(4)\"`"
            )),
        },
        "spring" => match params {
            None => Ok(EaseDecl::Spring(
                motion::SPRING_DEFAULT_STIFFNESS,
                motion::SPRING_DEFAULT_DAMPING,
            )),
            Some(inner) => {
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() != 2 {
                    return Err(format!(
                        "easing {t:?} needs `spring` or two numbers, e.g. `ease=\"spring(100,5)\"`"
                    ));
                }
                match (ease_number(parts[0]), ease_number(parts[1])) {
                    (Some(s), Some(d)) if s > 0.0 && d >= 0.0 => Ok(EaseDecl::Spring(s, d)),
                    _ => Err(format!(
                        "easing {t:?} needs `spring(stiffness,damping)` with a positive stiffness and a non-negative damping"
                    )),
                }
            }
        },
        "wiggle" => match params {
            None => Ok(EaseDecl::Wiggle(motion::WIGGLE_DEFAULT_COUNT)),
            Some(inner) => match ease_count(&inner) {
                Some(n) => Ok(EaseDecl::Wiggle(n)),
                _ => Err(format!(
                    "easing {t:?} needs `wiggle` or a positive whole wiggle count, e.g. `ease=\"wiggle(3)\"`"
                )),
            },
        },
        _ => Err(format!(
            "unknown easing {t:?}; write one of {}",
            EaseDecl::NAMED.join(", ")
        )),
    }
}

fn ease_number(text: &str) -> Option<f64> {
    let x: f64 = text.trim().parse().ok()?;
    x.is_finite().then_some(x)
}

fn ease_count(text: &str) -> Option<u32> {
    let x: f64 = text.trim().parse().ok()?;
    if x.is_finite() && x.fract() == 0.0 && x >= 1.0 && x <= u32::MAX as f64 {
        Some(x as u32)
    } else {
        None
    }
}

/// Canonical motion property for one authored name: hyphenated
/// spellings, the documented aliases, and every underscore form.
/// Later-sprint, filter, and sound properties are outside this slice
/// and refuse as unsupported naming their area; anything else is a
/// bad value listing the accepted set.
pub(crate) fn canon_property(raw: &str) -> Result<String, EditRefusal> {
    let canon = raw.trim().replace('_', "-");
    if MOTION_PROPERTIES.contains(&canon.as_str()) {
        return Ok(canon);
    }
    for (alias, target) in MOTION_ALIASES {
        if canon == alias {
            return Ok(target.to_string());
        }
    }
    if LATER_PROPERTIES.contains(&canon.as_str()) {
        return Err(refused(
            EditRefusalKind::Unsupported,
            E_SDK_UNSUPPORTED,
            format!(
                "sdk refused: property {raw:?} waits for later motion (one of {}); this SDK tweens only {}. Nothing was applied, previewed, or recorded.",
                LATER_PROPERTIES.join(", "),
                MOTION_PROPERTIES.join(", ")
            ),
        ));
    }
    if FILTER_PROPERTIES.contains(&canon.as_str()) {
        return Err(refused(
            EditRefusalKind::Unsupported,
            E_SDK_UNSUPPORTED,
            format!(
                "sdk refused: property {raw:?} is a Sprint 11 filter, not a motion property; this SDK tweens only {}. Nothing was applied, previewed, or recorded.",
                MOTION_PROPERTIES.join(", ")
            ),
        ));
    }
    if SOUND_PROPERTIES.contains(&canon.as_str()) {
        return Err(refused(
            EditRefusalKind::Unsupported,
            E_SDK_UNSUPPORTED,
            format!(
                "sdk refused: property {raw:?} rides sound envelopes on audio-bearing clips only; this SDK tweens visual properties ({}). Nothing was applied, previewed, or recorded.",
                MOTION_PROPERTIES.join(", ")
            ),
        ));
    }
    Err(refused(
        EditRefusalKind::BadValue,
        E_SDK_BAD_VALUE,
        format!(
            "sdk refused: property {raw:?} names nothing tweenable; write one of {}. Nothing was applied, previewed, or recorded.",
            MOTION_PROPERTIES.join(", ")
        ),
    ))
}

/// Properties whose tween endpoints are colors, parsed with the same
/// `#rgb`/`#rrggbb` vocabulary `check` accepts.
pub(crate) fn is_color_property(property: &str) -> bool {
    matches!(property, "fill" | "border-color")
}

/// Parse one authored number: finite, else a bad value naming the key.
pub(crate) fn parse_num(raw: &str, key: &str) -> Result<f64, EditRefusal> {
    match raw.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => Ok(x),
        _ => Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: `{key}` value {raw:?} is not a finite number; write a plain number. Nothing was applied, previewed, or recorded."
            ),
        )),
    }
}

/// Parse one authored color exactly like `check`.
pub(crate) fn parse_rgb(raw: &str, key: &str) -> Result<Rgb, EditRefusal> {
    notation::model::parse_color(raw).ok_or_else(|| {
        refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: `{key}` color {raw:?} is not `#rgb` or `#rrggbb`; write a hex color. Nothing was applied, previewed, or recorded."
            ),
        )
    })
}

/// Parse one `font-family` try-order list: comma-separated, every entry
/// non-empty, order preserved, never resolved or substituted here.
pub(crate) fn parse_font_family(raw: &str) -> Result<Vec<String>, EditRefusal> {
    let names: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
    if names.iter().any(|s| s.is_empty()) {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: `font-family` list {raw:?} carries an empty entry; write comma-separated family names like `\"Alpha, Beta\"`. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(names)
}

/// Resolve one clip reference against the staged clips: `id` first,
/// then index. An `id` that names nothing refuses even with an index
/// present; an index past the end refuses; neither set refuses.
pub(crate) fn resolve_clip(clips: &[Clip], target: &ClipRef) -> Result<usize, EditRefusal> {
    if let Some(id) = &target.id {
        if let Some(i) = clips.iter().position(|c| c.id() == Some(id.as_str())) {
            return Ok(i);
        }
        return Err(refused(
            EditRefusalKind::UnknownTarget,
            E_SDK_UNKNOWN_TARGET,
            format!(
                "sdk refused: {target} names no clip in the open composition; pass a declared `id` or a valid index. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    match target.index {
        Some(i) if i < clips.len() => Ok(i),
        Some(i) => Err(refused(
            EditRefusalKind::UnknownTarget,
            E_SDK_UNKNOWN_TARGET,
            format!(
                "sdk refused: clip index {i} is past the open composition holding {} clip(s); pass an index from 0 to {}. Nothing was applied, previewed, or recorded.",
                clips.len(),
                clips.len().saturating_sub(1)
            ),
        )),
        None => Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            "sdk refused: the clip target names neither `id` nor `index`; reference a clip by declared `id` or document index. Nothing was applied, previewed, or recorded.".to_string(),
        )),
    }
}

/// Prefix of the editor note carrying Layers group membership (Sprint
/// 18, T004 / CAP-004): a grouped clip's `label` reads `group:<name>`.
/// Notes ride along with no render effect, so grouping re-renders
/// byte-identically while the Layers tree, canvas, and timeline keep
/// selecting the same keys.
pub const GROUP_LABEL_PREFIX: &str = "group:";

/// The editor note marking membership in Layers group `name`.
pub fn group_label_for(name: &str) -> String {
    format!("{GROUP_LABEL_PREFIX}{name}")
}

/// The Layers group name of one editor note, when the note carries
/// group membership.
pub fn group_name_of_label(label: &str) -> Option<&str> {
    label
        .strip_prefix(GROUP_LABEL_PREFIX)
        .filter(|name| !name.is_empty())
}

/// One group member's prior editor note, restored on undo.
#[derive(Debug, Clone)]
pub struct GroupLabelRestore {
    /// Clip reference, resolved `id`-first like every other revert.
    pub target: ClipRef,
    /// The exact prior note (`None` where the slot was note-free).
    pub label: Option<String>,
}

/// Motion directives whose clip target sits inside the document-index
/// range `lo..=hi`, in `tween`/`set`/`key` document order: a move
/// shuffling any of these indices re-points the directive at another
/// clip, so the move refuses naming them instead of detaching motion.
pub(crate) fn motion_targets_in(comp: &Composition, lo: usize, hi: usize) -> Vec<String> {
    let mut out = Vec::new();
    for (index, tween) in comp.tweens.iter().enumerate() {
        if (lo..=hi).contains(&tween.target) {
            out.push(format!("tween {index} (clip index {})", tween.target));
        }
    }
    for (index, set) in comp.sets.iter().enumerate() {
        if (lo..=hi).contains(&set.target) {
            out.push(format!("set {index} (clip index {})", set.target));
        }
    }
    for (index, key) in comp.keys.iter().enumerate() {
        if (lo..=hi).contains(&key.target) {
            out.push(format!("key {index} (clip index {})", key.target));
        }
    }
    out
}

/// Distinct Layers group names of the staged clips, in first-member
/// document order.
pub(crate) fn staged_group_names(comp: &Composition) -> Vec<String> {
    let mut names = Vec::new();
    for clip in &comp.clips {
        if let Some(label) = clip.label() {
            if let Some(name) = group_name_of_label(label) {
                if !names.iter().any(|n| n == name) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Mutable label slot of one staged clip.
pub(crate) fn clip_label_mut(clip: &mut Clip) -> &mut Option<String> {
    match clip {
        Clip::Text(c) => &mut c.label,
        Clip::Video(c) => &mut c.label,
        Clip::Box(c) => &mut c.label,
        Clip::Flex(c) => &mut c.label,
        Clip::Image(c) => &mut c.label,
        Clip::Path(c) => &mut c.label,
        Clip::Svg(c) => &mut c.label,
        Clip::Chart(c) => &mut c.label,
        Clip::Map(c) => &mut c.label,
        Clip::Scene3d(c) => &mut c.label,
        Clip::Caption(c) => &mut c.label,
        Clip::Lottie(c) => &mut c.label,
        Clip::Audio(c) => &mut c.label,
        Clip::Nest(c) => &mut c.label,
    }
}

/// Footage-style media window of one staged clip (Sprint 15, T007):
/// (`trim_start`, `trim_end`, `rate`, looping). `video`, `audio`, and
/// `nest` clips consume a source window; every other kind returns
/// `None` and refuses a `trim_start` outright.
pub(crate) fn clip_media_window(clip: &Clip) -> Option<(f64, Option<f64>, f64, bool)> {
    match clip {
        Clip::Video(c) => Some((c.trim_start, c.trim_end, c.rate, c.loop_clip)),
        Clip::Audio(c) => Some((c.trim_start, c.trim_end, c.rate, false)),
        Clip::Nest(c) => Some((c.trim_start, c.trim_end, c.rate, false)),
        _ => None,
    }
}

/// Mutable file-window slots of one staged footage-style clip
/// (`trim_start`, `trim_end`); `None` on every other kind.
fn clip_media_window_mut(clip: &mut Clip) -> Option<(&mut f64, &mut Option<f64>)> {
    match clip {
        Clip::Video(c) => Some((&mut c.trim_start, &mut c.trim_end)),
        Clip::Audio(c) => Some((&mut c.trim_start, &mut c.trim_end)),
        Clip::Nest(c) => Some((&mut c.trim_start, &mut c.trim_end)),
        _ => None,
    }
}

/// Overwrite the stable `id` of one staged clip.
pub(crate) fn clip_set_id(clip: &mut Clip, id: Option<String>) {
    match clip {
        Clip::Text(c) => c.id = id,
        Clip::Video(c) => c.id = id,
        Clip::Box(c) => c.id = id,
        Clip::Flex(c) => c.id = id,
        Clip::Image(c) => c.id = id,
        Clip::Path(c) => c.id = id,
        Clip::Svg(c) => c.id = id,
        Clip::Chart(c) => c.id = id,
        Clip::Map(c) => c.id = id,
        Clip::Scene3d(c) => c.id = id,
        Clip::Caption(c) => c.id = id,
        Clip::Lottie(c) => c.id = id,
        Clip::Audio(c) => c.id = id,
        Clip::Nest(c) => c.id = id,
    }
}

/// Overwrite the visibility switch of one staged clip.
fn clip_set_hidden(clip: &mut Clip, hidden: bool) {
    match clip {
        Clip::Text(c) => c.hidden = hidden,
        Clip::Video(c) => c.hidden = hidden,
        Clip::Box(c) => c.hidden = hidden,
        Clip::Flex(c) => c.hidden = hidden,
        Clip::Image(c) => c.hidden = hidden,
        Clip::Path(c) => c.hidden = hidden,
        Clip::Svg(c) => c.hidden = hidden,
        Clip::Chart(c) => c.hidden = hidden,
        Clip::Map(c) => c.hidden = hidden,
        Clip::Scene3d(c) => c.hidden = hidden,
        Clip::Caption(c) => c.hidden = hidden,
        Clip::Lottie(c) => c.hidden = hidden,
        Clip::Audio(c) => c.hidden = hidden,
        Clip::Nest(c) => c.hidden = hidden,
    }
}

/// Path data of one staged clip, when it is a path clip.
pub(crate) fn clip_path_d(clip: &Clip) -> Option<&str> {
    match clip {
        Clip::Path(c) => Some(c.d.as_str()),
        _ => None,
    }
}

pub(crate) fn clip_path_d_mut(clip: &mut Clip) -> Option<&mut String> {
    match clip {
        Clip::Path(c) => Some(&mut c.d),
        _ => None,
    }
}

/// Validate one candidate `id`: non-empty and unique across the staged
/// clips. `check` requires project-wide uniqueness; the staged set is
/// the composition the SDK sees.
pub(crate) fn check_new_id(
    clips: &[Clip],
    id: &Option<String>,
    what: &str,
) -> Result<(), EditRefusal> {
    let Some(id) = id else { return Ok(()) };
    if id.trim().is_empty() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the added {what} carries an empty `id`; write a non-empty stable name or omit it. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    if clips.iter().any(|c| c.id() == Some(id.as_str())) {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: `id` {id:?} is already taken in the open composition; `id` values stay unique. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Check one motion value against the per-property ranges `check`
/// enforces (opacity, sizes, radii): out-of-range refuses naming the
/// range, never clamped silently.
pub(crate) fn check_motion_value(property: &str, key: &str, v: f64) -> Result<(), EditRefusal> {
    let range: Option<&str> = match property {
        "opacity" => {
            if !(0.0..=1.0).contains(&v) {
                Some("0 (transparent) to 1 (opaque)")
            } else {
                None
            }
        }
        "width" | "height" | "font-size" => {
            if v <= 0.0 {
                Some("a positive number of pixels")
            } else {
                None
            }
        }
        "radius" | "letter-spacing" | "border-width" | "blur" => {
            if v < 0.0 {
                Some("0 pixels and up")
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(runs) = range {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: `{key}` value {v} for property {property:?} runs {runs}; write a value in range. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Check one motion window: finite start at or after zero, finite
/// positive duration.
pub(crate) fn check_motion_window(
    start: f64,
    duration: f64,
    what: &str,
) -> Result<(), EditRefusal> {
    check_window(start, duration, what)
}

/// Check one clip window: finite start at or after zero, finite
/// positive duration.
pub(crate) fn check_window(start: f64, duration: f64, what: &str) -> Result<(), EditRefusal> {
    if !start.is_finite() || start < 0.0 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the {what} starts at {start:?}; write a finite start at or after 0. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    if !duration.is_finite() || duration <= 0.0 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the {what} lasts {duration:?}; write a finite positive duration. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Is the clip an audio track (carries sound envelopes, not visual
/// motion in this slice)?
fn is_audio_clip(clip: &Clip) -> bool {
    matches!(clip, Clip::Audio(_))
}

/// Build one added element into a full clip: every check runs before
/// the caller mutates anything, so a bad value leaves the staged set
/// untouched.
fn build_new_clip(clips: &[Clip], element: &NewElement) -> Result<Clip, EditRefusal> {
    match element {
        NewElement::Text {
            id,
            label,
            value,
            start,
            duration,
            x,
            y,
            size,
            color,
            font_family,
        } => {
            check_new_id(clips, id, "text element")?;
            check_window(*start, *duration, "text element")?;
            for (key, v) in [("x", *x), ("y", *y)] {
                if !v.is_finite() {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: `{key}` value {v:?} is not finite; write a plain number. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            let size_v = parse_num(&size.to_string(), "size")?;
            if size_v <= 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: `size` value {size_v} runs a positive number of pixels; write a positive size. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let color_v = parse_rgb(color, "color")?;
            let families = font_family.as_deref().map(parse_font_family).transpose()?;
            Ok(Clip::Text(TextClip {
                asserts: MotionAsserts::default(),
                value: value.clone(),
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Graphics,
                label: label.clone(),
                start_after: None,
                x: *x,
                y: *y,
                size: size_v,
                color: color_v,
                align: Align::Left,
                wrap: Wrap::Off,
                style: FontStyle::Regular,
                order: 0,
                line_height: None,
                letter_spacing: None,
                font_family: families,
                direction: TextDirection::Ltr,
                spans: Vec::new(),
                split: None,
                typewriter: false,
                count_from: None,
                count_to: None,
                decimals: 0,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                hidden: false,
                line: 0,
                column: 0,
            }))
        }
        NewElement::Box {
            id,
            label,
            start,
            duration,
            x,
            y,
            width,
            height,
            fill,
        } => {
            check_new_id(clips, id, "box element")?;
            check_window(*start, *duration, "box element")?;
            // The struct already holds numbers (JSON), so only range
            // rules apply beyond finiteness.
            let w = *width;
            let h = *height;
            if !w.is_finite() || w <= 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: `width` value {w:?} runs a positive number of pixels; write a positive width. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            if !h.is_finite() || h <= 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: `height` value {h:?} runs a positive number of pixels; write a positive height. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            // Windows still need their start/duration: added boxes run
            // the composition window when unwritten elsewhere, so the
            // caller supplies them through the position default below.
            let fill_v = parse_rgb(fill, "fill")?;
            for (key, v) in [("x", *x), ("y", *y)] {
                if !v.is_finite() {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: `{key}` value {v:?} is not finite; write a plain number. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            Ok(Clip::Box(BoxClip {
                asserts: MotionAsserts::default(),
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Graphics,
                label: label.clone(),
                start_after: None,
                x: *x,
                y: *y,
                width: w,
                height: h,
                fill: fill_v,
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
                order: 0,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                hidden: false,
                line: 0,
                column: 0,
            }))
        }
        NewElement::Path {
            id,
            label,
            start,
            duration,
            x,
            y,
            d,
            fill,
            fill_rule,
        } => {
            check_new_id(clips, id, "path element")?;
            check_window(*start, *duration, "path element")?;
            if motion::path::parse(d).is_err() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: `d` value {d:?} parses no path commands; write SVG-subset commands (`M`, `L`, `C`, `Q`, `A`, `Z`). Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let fill_v = parse_rgb(fill, "fill")?;
            let rule = match fill_rule.as_deref() {
                None => FillRule::NonZero,
                Some(s) => FillRule::parse(&s.trim().replace('_', "-")).ok_or_else(|| {
                    refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: `fill-rule` value {s:?} names no rule; write `nonzero` or `evenodd`. Nothing was applied, previewed, or recorded."
                        ),
                    )
                })?,
            };
            for (key, v) in [("x", *x), ("y", *y)] {
                if !v.is_finite() {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: `{key}` value {v:?} is not finite; write a plain number. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            Ok(Clip::Path(PathClip {
                asserts: MotionAsserts::default(),
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Graphics,
                label: label.clone(),
                start_after: None,
                x: *x,
                y: *y,
                d: d.clone(),
                fill: fill_v,
                fill_rule: rule,
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
                order: 0,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                hidden: false,
                line: 0,
                column: 0,
            }))
        }
        NewElement::Nest {
            id,
            label,
            target,
            start,
            duration,
        } => {
            check_new_id(clips, id, "nest element")?;
            check_window(*start, *duration, "nest element")?;
            if target.trim().is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    "sdk refused: the added nest element names no composition; write `target` as a composition `name` or `id`. Nothing was applied, previewed, or recorded."
                        .to_string(),
                ));
            }
            // The full source window plays by default (`trim-end`
            // unset); the caller (the Studio Compositions panel) picks
            // the film window and guards existence, cycles, and depth
            // against the whole project before applying.
            Ok(Clip::Nest(NestClip {
                asserts: MotionAsserts::default(),
                target: target.clone(),
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Graphics,
                label: label.clone(),
                start_after: None,
                trim_start: 0.0,
                trim_end: None,
                rate: 1.0,
                order: 0,
                hidden: false,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                line: 0,
                column: 0,
            }))
        }
        NewElement::Image {
            id,
            label,
            path,
            start,
            duration,
        } => {
            check_new_id(clips, id, "image element")?;
            check_window(*start, *duration, "image element")?;
            let rel = check_media_path(path, "image element")?;
            // The caller (the Studio Assets panel) guards that the
            // file exists; the open session alone cannot see sibling
            // files. Unset sizes mean the composition canvas, exactly
            // like an unwritten `width`/`height`.
            Ok(Clip::Image(ImageClip {
                asserts: MotionAsserts::default(),
                path: rel,
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Graphics,
                label: label.clone(),
                start_after: None,
                x: 0.0,
                y: 0.0,
                width: None,
                height: None,
                fit: Fit::Contain,
                placement: Placement::center(),
                order: 0,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                hidden: false,
                line: 0,
                column: 0,
            }))
        }
        NewElement::Video {
            id,
            label,
            path,
            start,
            duration,
        } => {
            check_new_id(clips, id, "video element")?;
            check_window(*start, *duration, "video element")?;
            let rel = check_media_path(path, "video element")?;
            // The full source window plays by default (`trim-end`
            // unset); the caller (the Studio Assets panel) picks the
            // film window and guards that the file exists.
            Ok(Clip::Video(VideoClip {
                asserts: MotionAsserts::default(),
                path: rel,
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Video,
                label: label.clone(),
                duration_inferred: false,
                start_after: None,
                fit: Fit::Contain,
                order: 0,
                volume_db: 0.0,
                fade_in: 0.0,
                fade_out: 0.0,
                trim_start: 0.0,
                trim_end: None,
                rate: 1.0,
                loop_clip: false,
                turn: Turn3D::flat(),
                filter_blur: 0.0,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                crossfade: Crossfade::default(),
                duck_under: None,
                fx: SoundFx::default(),
                waveform: WaveformDraw::default(),
                hidden: false,
                line: 0,
                column: 0,
            }))
        }
        NewElement::Audio {
            id,
            label,
            path,
            start,
            duration,
        } => {
            check_new_id(clips, id, "audio element")?;
            check_window(*start, *duration, "audio element")?;
            let rel = check_media_path(path, "audio element")?;
            // The full source window plays by default (`trim-end`
            // unset); the caller (the Studio Assets panel) picks the
            // film window and guards that the file exists.
            Ok(Clip::Audio(AudioClip {
                path: rel,
                start: *start,
                duration: *duration,
                id: id.clone(),
                track: TrackKind::Audio,
                label: label.clone(),
                start_after: None,
                trim_start: 0.0,
                trim_end: None,
                rate: 1.0,
                volume_db: 0.0,
                fade_in: 0.0,
                fade_out: 0.0,
                crossfade: Crossfade::default(),
                duck_under: None,
                fx: SoundFx::default(),
                waveform: WaveformDraw::default(),
                hidden: false,
                grade: notation::model::Grade::empty(),
                effects: Vec::new(),
                vfx: notation::model::VfxChain::empty(),
                line: 0,
                column: 0,
            }))
        }
    }
}

/// Check one added media path: a non-empty project-relative media
/// file path, stored trimmed. Whether the file exists is checked by
/// the caller (the Studio Assets panel), which reads the project
/// folder — the open session alone cannot see sibling files.
fn check_media_path(raw: &str, what: &str) -> Result<String, EditRefusal> {
    let rel = raw.trim().to_string();
    if rel.is_empty() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the added {what} names no media file; write `path` as a project-relative media path like `media/clip.mp4`. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(rel)
}

/// Resolve a tween/key endpoint: colors for `fill`/`border-color`,
/// range-checked numbers otherwise.
#[allow(clippy::too_many_arguments)]
fn resolve_endpoint(
    raw: &str,
    key: &str,
    property: &str,
    what: &str,
) -> Result<(Option<f64>, Option<Rgb>), EditRefusal> {
    if is_color_property(property) {
        Ok((None, Some(parse_rgb(raw, key)?)))
    } else {
        let v = parse_num(raw, key)?;
        check_motion_value(property, key, v).map_err(|_| {
            refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: the {what} `{key}` value {v} breaks the {property:?} range; write a value in range. Nothing was applied, previewed, or recorded."
                ),
            )
        })?;
        Ok((Some(v), None))
    }
}

/// Build one set directive: a held number over a window with no
/// easing. Colors hold no single number, so `fill` and
/// `border-color` refuse naming the `tween` or `key` that
/// interpolates them instead — exactly like `check`.
fn build_set(
    target: usize,
    property: String,
    value_raw: &str,
    start: f64,
    duration: f64,
) -> Result<SetDecl, EditRefusal> {
    if is_color_property(&property) {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: set property {property:?} holds a color; a `set` holds one number, so colors animate with `tween` or `key`. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    let value = parse_num(value_raw, "value")?;
    check_motion_value(&property, "value", value)?;
    check_motion_window(start, duration, "set")?;
    Ok(SetDecl {
        target,
        property,
        value,
        start,
        duration,
        line: 0,
        column: 0,
    })
}

/// Check one authored path string: it must parse, else the engine
/// cannot follow it. The trimmed text returns for storage, so the
/// persisted turn re-validates exactly what the maker wrote.
fn check_path_data(raw: &str, key: &str) -> Result<String, EditRefusal> {
    let text = raw.trim().to_string();
    if let Err(e) = motion::path::parse(&text) {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the engine cannot follow {key} {raw:?}: {e}. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(text)
}

/// Check one path morph destination: both paths parsed already, so a
/// structural difference refuses naming the first difference, never
/// faked with a partial morph.
fn check_path_morph(path: &str, path_to: Option<&str>) -> Result<(), EditRefusal> {
    let Some(dest) = path_to else {
        return Ok(());
    };
    let from = motion::path::parse(path).expect("travel path parses after validation");
    let to = motion::path::parse(dest).expect("morph destination parses after validation");
    if let Err(e) = motion::path::morph(&from, &to, 0.0) {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the engine cannot morph toward `path-to` {dest:?}: {e}. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Build one tween directive: kind/endpoints first, then easing and
/// window, before the caller mutates anything.
#[allow(clippy::too_many_arguments)]
fn build_tween(
    target: usize,
    property: String,
    kind_raw: &str,
    from_raw: &Option<String>,
    to_raw: &Option<String>,
    start: f64,
    duration: f64,
    ease_raw: &str,
) -> Result<TweenDecl, EditRefusal> {
    let kind = TweenKind::parse(kind_raw.trim()).ok_or_else(|| {
        refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: tween kind {kind_raw:?} names no motion; write `to`, `from`, or `from-to`. Nothing was applied, previewed, or recorded."
            ),
        )
    })?;
    let need_from = matches!(kind, TweenKind::From | TweenKind::FromTo);
    let need_to = matches!(kind, TweenKind::To | TweenKind::FromTo);
    if need_from && from_raw.is_none() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: a `{}` tween needs `from`; write the value motion starts at. Nothing was applied, previewed, or recorded.",
                kind.name()
            ),
        ));
    }
    if need_to && to_raw.is_none() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: a `{}` tween needs `to`; write the value motion ends at. Nothing was applied, previewed, or recorded.",
                kind.name()
            ),
        ));
    }
    if !need_from && from_raw.is_some() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: a `{}` tween takes no `from`; remove it or write kind `from-to`. Nothing was applied, previewed, or recorded.",
                kind.name()
            ),
        ));
    }
    if !need_to && to_raw.is_some() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: a `{}` tween takes no `to`; remove it or write kind `from-to`. Nothing was applied, previewed, or recorded.",
                kind.name()
            ),
        ));
    }
    let (from, from_color) = match from_raw {
        Some(raw) => {
            let (v, c) = resolve_endpoint(raw, "from", &property, "tween")?;
            (v, c)
        }
        None => (None, None),
    };
    let (to, to_color) = match to_raw {
        Some(raw) => {
            let (v, c) = resolve_endpoint(raw, "to", &property, "tween")?;
            (v, c)
        }
        None => (None, None),
    };
    let ease = parse_ease(ease_raw).map_err(|why| {
        refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!("sdk refused: {why}. Nothing was applied, previewed, or recorded."),
        )
    })?;
    check_motion_window(start, duration, "tween")?;
    Ok(TweenDecl {
        target,
        property,
        kind,
        from,
        to,
        from_color,
        to_color,
        start,
        duration,
        ease,
        stagger: 0.0,
        stagger_from: notation::model::StaggerOrigin::Start,
        stagger_grid: None,
        repeat: 1,
        yoyo: false,
        repeat_delay: 0.0,
        path: None,
        path_to: None,
        orient: false,
        expr: None,
        line: 0,
        column: 0,
    })
}

/// Locate one key by index (Sprint 16, T007): an unknown index
/// refuses naming how many keys the open composition holds.
fn key_index(comp: &Composition, key: usize) -> Result<usize, EditRefusal> {
    if key >= comp.keys.len() {
        return Err(refused(
            EditRefusalKind::UnknownTarget,
            E_SDK_UNKNOWN_TARGET,
            format!(
                "sdk refused: key index {key} names nothing; the open composition holds {} key(s). Nothing was applied, previewed, or recorded.",
                comp.keys.len()
            ),
        ));
    }
    Ok(key)
}

/// One mutable keyframe stop with its neighbor instants copied out
/// (Sprint 16, T007): the slot plus the strictly-rising bounds the
/// retime must keep. Missing neighbors read as unbounded infinities;
/// callers gate on the stop position instead of reading them blind.
struct KeyStopSlot<'a> {
    slot: &'a mut KeyStop,
    prev_at: f64,
    next_at: f64,
    stop_count: usize,
}

/// Locate one mutable keyframe stop (Sprint 16, T007): an unknown key
/// or stop refuses naming the composition count.
fn key_stop_mut(
    comp: &mut Composition,
    key: usize,
    stop: usize,
) -> Result<KeyStopSlot<'_>, EditRefusal> {
    let index = key_index(comp, key)?;
    let stops = &comp.keys[index].stops;
    if stop >= stops.len() {
        return Err(refused(
            EditRefusalKind::UnknownTarget,
            E_SDK_UNKNOWN_TARGET,
            format!(
                "sdk refused: key {key} stop {stop} names nothing; key {key} holds {} stop(s). Nothing was applied, previewed, or recorded.",
                stops.len()
            ),
        ));
    }
    let prev_at = if stop > 0 {
        stops[stop - 1].at
    } else {
        f64::NEG_INFINITY
    };
    let next_at = if stop + 1 < stops.len() {
        stops[stop + 1].at
    } else {
        f64::INFINITY
    };
    let stop_count = stops.len();
    Ok(KeyStopSlot {
        slot: &mut comp.keys[index].stops[stop],
        prev_at,
        next_at,
        stop_count,
    })
}

/// Locate one mutable easing slot (Sprint 16, T007): the key-level
/// fallback when `stop` is `None`, else the easing of the segment
/// leaving stop `stop`.
fn key_ease_slot(
    comp: &mut Composition,
    key: usize,
    stop: Option<usize>,
) -> Result<&mut Option<EaseDecl>, EditRefusal> {
    let index = key_index(comp, key)?;
    match stop {
        None => Ok(&mut comp.keys[index].ease),
        Some(s) => {
            if s >= comp.keys[index].stops.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: key {key} stop {s} names nothing; key {key} holds {} stop(s). Nothing was applied, previewed, or recorded.",
                        comp.keys[index].stops.len()
                    ),
                ));
            }
            Ok(&mut comp.keys[index].stops[s].ease)
        }
    }
}

/// Snapshot one keyframe stop (Sprint 16, T007): the key index with an
/// exact copy of the stop, for stop-removal inverses.
fn key_stop_snapshot(
    comp: &Composition,
    key: usize,
    stop: usize,
) -> Result<(usize, KeyStop), EditRefusal> {
    let index = key_index(comp, key)?;
    comp.keys[index]
        .stops
        .get(stop)
        .cloned()
        .map(|snapshot| (index, snapshot))
        .ok_or_else(|| {
            refused(
                EditRefusalKind::UnknownTarget,
                E_SDK_UNKNOWN_TARGET,
                format!(
                    "sdk refused: key {key} stop {stop} names nothing; key {key} holds {} stop(s). Nothing was applied, previewed, or recorded.",
                    comp.keys[index].stops.len()
                ),
            )
        })
}

/// Build one key directive: at least two stops in strictly rising
/// time order with per-property values, before the caller mutates.
fn build_key(
    target: usize,
    property: String,
    stops_raw: &[KeyStopInput],
    ease_raw: &Option<String>,
) -> Result<KeyDecl, EditRefusal> {
    if stops_raw.len() < 2 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the key carries {} stop(s); write at least two stops in rising time order. Nothing was applied, previewed, or recorded.",
                stops_raw.len()
            ),
        ));
    }
    let ease = ease_raw
        .as_deref()
        .map(parse_ease)
        .transpose()
        .map_err(|why| {
            refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!("sdk refused: {why}. Nothing was applied, previewed, or recorded."),
            )
        })?;
    let mut stops = Vec::with_capacity(stops_raw.len());
    let mut prev_at: Option<f64> = None;
    for (i, stop) in stops_raw.iter().enumerate() {
        if !stop.at.is_finite() || stop.at < 0.0 {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: key stop {i} sits at {:?}; write a finite instant at or after 0. Nothing was applied, previewed, or recorded.",
                    stop.at
                ),
            ));
        }
        if let Some(prev) = prev_at {
            if stop.at <= prev {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key stop {i} sits at {} which never rises past stop {} at {prev}; write stops in strictly rising time order. Nothing was applied, previewed, or recorded.",
                        stop.at,
                        i - 1
                    ),
                ));
            }
        }
        prev_at = Some(stop.at);
        let (value, value_color) = if is_color_property(&property) {
            (0.0, Some(parse_rgb(&stop.value, "value")?))
        } else {
            let v = parse_num(&stop.value, "value")?;
            check_motion_value(&property, "value", v).map_err(|_| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key stop {i} value {v} breaks the {property:?} range; write a value in range. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
            (v, None)
        };
        let stop_ease = stop
            .ease
            .as_deref()
            .map(parse_ease)
            .transpose()
            .map_err(|why| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key stop {i} {why}. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
        stops.push(KeyStop {
            at: stop.at,
            value,
            value_color,
            path: None,
            ease: stop_ease,
            line: 0,
            column: 0,
        });
    }
    Ok(KeyDecl {
        target,
        property,
        ease,
        stops,
        stagger: 0.0,
        stagger_from: notation::model::StaggerOrigin::Start,
        stagger_grid: None,
        repeat: 1,
        yoyo: false,
        repeat_delay: 0.0,
        orient: false,
        line: 0,
        column: 0,
    })
}

/// Whether a clip carries sound envelopes (Sprint 18, T007 / CAP-007):
/// `audio` tracks and `video` clips, exactly the clips `check`
/// accepts `volume`/`pan` directives on. Every other kind is silent
/// for lane purposes and refuses lane edits naming its kind.
fn is_sound_bearing_clip(clip: &Clip) -> bool {
    matches!(clip, Clip::Audio(_) | Clip::Video(_))
}

/// Canonical lane property for one authored name (Sprint 18, T007):
/// the hyphenated sound-envelope spelling with underscores folded,
/// exactly like `check`. Anything else refuses naming `volume` and
/// `pan`.
fn canon_lane_property(raw: &str) -> Result<String, EditRefusal> {
    let canon = raw.trim().replace('_', "-");
    if SOUND_PROPERTIES.contains(&canon.as_str()) {
        return Ok(canon);
    }
    Err(refused(
        EditRefusalKind::BadValue,
        E_SDK_BAD_VALUE,
        format!(
            "sdk refused: lane property {raw:?} names no automation lane; write `volume` or `pan`. Nothing was applied, previewed, or recorded."
        ),
    ))
}

/// Check one lane value against its envelope range (Sprint 18, T007):
/// `volume` rides 0 (silence) to 2 (twice unity), `pan` rides −1
/// (left) to 1 (right) — the same ranges `check` enforces, so a
/// refused value names the range instead of clamping silently.
fn check_lane_value(property: &str, key: &str, v: f64) -> Result<(), EditRefusal> {
    let ok = match property {
        "volume" => (0.0..=2.0).contains(&v),
        "pan" => (-1.0..=1.0).contains(&v),
        _ => true,
    };
    if !ok {
        let runs = match property {
            "volume" => "0 (silence) to 2 (twice unity)",
            _ => "-1 (left) to 1 (right)",
        };
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: lane `{key}` value {v} for property {property:?} runs {runs}; write a value in range. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Check one lane window: finite edges at or after zero with the end
/// strictly past the start, else the range names nothing to rewrite.
fn check_lane_window(start: f64, end: f64) -> Result<(), EditRefusal> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the lane window {start}..{end} is not a finite range with the end past the start; write `start` at or after 0 and `end` past it. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    Ok(())
}

/// Resolve one lane target: the clip index plus the refusal when the
/// clip carries no sound. Silent clips (text, boxes, images, paths,
/// svg, flex, nest) name their kind in the refusal, never guessed
/// past.
fn resolve_lane_clip(clips: &[Clip], target: &ClipRef) -> Result<usize, EditRefusal> {
    let idx = resolve_clip(clips, target)?;
    if !is_sound_bearing_clip(&clips[idx]) {
        return Err(refused(
            EditRefusalKind::Unsupported,
            E_SDK_UNSUPPORTED,
            format!(
                "sdk refused: {target} is a {}; automation lanes ride sound-bearing clips (`audio` and `video`) only, so the range over silence rewrites nothing. Nothing was applied, previewed, or recorded.",
                clips[idx].what()
            ),
        ));
    }
    Ok(idx)
}

/// Read one lane value at an instant (Sprint 18, T007): linear
/// interpolation between the neighbor stops of every key on the
/// lane, edge hold outside the stops, the property's own value
/// (`volume` unity, `pan` center) on a keyless lane. Eased segments
/// evaluate per sample at render time; the rack reads the straight
/// line so proposed points stay predictable and documented.
fn lane_value_at(comp: &Composition, target: usize, property: &str, at: f64) -> f64 {
    let mut stops: Vec<(f64, f64)> = Vec::new();
    for key in comp
        .keys
        .iter()
        .filter(|k| k.target == target && k.property == property)
    {
        for stop in &key.stops {
            stops.push((stop.at, stop.value));
        }
    }
    if stops.is_empty() {
        return notation::model::sound_own_value(property);
    }
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if at <= stops[0].0 {
        return stops[0].1;
    }
    if at >= stops[stops.len() - 1].0 {
        return stops[stops.len() - 1].1;
    }
    for pair in stops.windows(2) {
        if pair[0].0 <= at && at <= pair[1].0 {
            let span = pair[1].0 - pair[0].0;
            if span <= 0.0 {
                return pair[1].1;
            }
            let f = (at - pair[0].0) / span;
            return pair[0].1 + f * (pair[1].1 - pair[0].1);
        }
    }
    stops[stops.len() - 1].1
}

/// Parse one authored lane number: finite, in the property range,
/// else the refusal names the range.
fn parse_lane_value(raw: &str, key: &str, property: &str) -> Result<f64, EditRefusal> {
    let v = parse_num(raw, key)?;
    check_lane_value(property, key, v)?;
    Ok(v)
}

/// Collect the in-window stops of one lane (Sprint 18, T007): every
/// `(key index, stop)` of the target's `property` keys with
/// `start <= at <= end`, in key order. Empty means the range is
/// silent and refuses instead of inventing points.
fn lane_stops_in(
    comp: &Composition,
    target: usize,
    property: &str,
    start: f64,
    end: f64,
) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (ki, key) in comp.keys.iter().enumerate() {
        if key.target != target || key.property != property {
            continue;
        }
        for (si, stop) in key.stops.iter().enumerate() {
            if start <= stop.at && stop.at <= end {
                found.push((ki, si));
            }
        }
    }
    found
}

/// Copy one lane range (Sprint 18, T007 / CAP-007): the `volume`/`pan`
/// stops of the sound-bearing target inside `[start, end]` as authored
/// stop inputs — instant, value, and segment easing — so pasting them
/// back rewrites the same points exactly. The copy reads project
/// truth and stages nothing; a silent target or an empty range
/// refuses with its cause instead of returning invented points.
pub fn copy_lane_range(
    comp: &Composition,
    target: &ClipRef,
    property_raw: &str,
    start: f64,
    end: f64,
) -> Result<Vec<KeyStopInput>, EditRefusal> {
    let idx = resolve_lane_clip(&comp.clips, target)?;
    let property = canon_lane_property(property_raw)?;
    check_lane_window(start, end)?;
    let mut stops: Vec<KeyStopInput> = Vec::new();
    for key in comp
        .keys
        .iter()
        .filter(|k| k.target == idx && k.property == property)
    {
        for stop in &key.stops {
            if start <= stop.at && stop.at <= end {
                stops.push(KeyStopInput {
                    at: stop.at,
                    value: stop.value.to_string(),
                    ease: stop.ease.map(|e| e.name()),
                });
            }
        }
    }
    if stops.is_empty() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the lane `{property}` on {target} holds no stops across {start}..{end}; the range is empty, so there is nothing to copy. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    stops.sort_by(|a, b| a.at.partial_cmp(&b.at).unwrap_or(std::cmp::Ordering::Equal));
    Ok(stops)
}

/// Apply operations to a copy of one composition (Sprint 18, T007 /
/// CAP-007): clones `base`, runs every operation through the same
/// forward path [`SdkSession::apply`](crate::SdkSession::apply) uses,
/// and returns the modified copy — or the first refusal with nothing
/// returned. The base stays untouched either way, so preset audition
/// measures the real mix of the modified copy while the project keeps
/// its truth. Inverses are computed and dropped: audition previews,
/// never records.
pub fn apply_ops_to_composition(
    base: &Composition,
    ops: &[EditOp],
) -> Result<Composition, EditRefusal> {
    let mut next = base.clone();
    for op in ops {
        let _ = apply_forward(&mut next, op)?;
    }
    Ok(next)
}

/// Build the fresh stops of one lane rewrite (Sprint 18, T007): the
/// replacement points for `[start, end]` per `edit`, with every value
/// parsed and range-checked before the caller mutates anything. Reads
/// boundary values from the lane itself, so omitted endpoints ride
/// the current mix instead of invented levels.
#[allow(clippy::too_many_arguments)]
fn build_lane_rewrite(
    comp: &Composition,
    target: usize,
    property: &str,
    start: f64,
    end: f64,
    edit: &LaneEdit,
    in_window: &[(usize, usize)],
) -> Result<Vec<KeyStop>, EditRefusal> {
    let stop = |at: f64, value: f64, ease: Option<EaseDecl>| KeyStop {
        at,
        value,
        value_color: None,
        path: None,
        ease,
        line: 0,
        column: 0,
    };
    match edit {
        LaneEdit::Ramp { from, to } => {
            let v0 = match from {
                Some(raw) => parse_lane_value(raw, "from", property)?,
                None => lane_value_at(comp, target, property, start),
            };
            let v1 = match to {
                Some(raw) => parse_lane_value(raw, "to", property)?,
                None => lane_value_at(comp, target, property, end),
            };
            Ok(vec![stop(start, v0, None), stop(end, v1, None)])
        }
        LaneEdit::Swell { peak } => {
            let p = parse_lane_value(peak, "peak", property)?;
            let mid = (start + end) / 2.0;
            Ok(vec![
                stop(start, lane_value_at(comp, target, property, start), None),
                stop(mid, p, None),
                stop(end, lane_value_at(comp, target, property, end), None),
            ])
        }
        LaneEdit::Dip { trough } => {
            let t = parse_lane_value(trough, "trough", property)?;
            let mid = (start + end) / 2.0;
            Ok(vec![
                stop(start, lane_value_at(comp, target, property, start), None),
                stop(mid, t, None),
                stop(end, lane_value_at(comp, target, property, end), None),
            ])
        }
        LaneEdit::Simplify { tolerance } => {
            if !tolerance.is_finite() || *tolerance < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: simplify tolerance {tolerance:?} is not finite and non-negative; write 0 for exact endpoints only, more to forgive drift. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let mut ordered: Vec<(f64, f64, Option<EaseDecl>)> = in_window
                .iter()
                .map(|(ki, si)| {
                    let s = &comp.keys[*ki].stops[*si];
                    (s.at, s.value, s.ease)
                })
                .collect();
            ordered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            if ordered.len() < 3 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: simplify needs at least three stops across {start}..{end}; the lane holds {} there, so there is nothing to reduce. Nothing was applied, previewed, or recorded.",
                        ordered.len()
                    ),
                ));
            }
            let (first_at, first_v, _) = ordered[0];
            let (last_at, last_v, _) = ordered[ordered.len() - 1];
            let span = last_at - first_at;
            let mut kept: Vec<(f64, f64, Option<EaseDecl>)> = Vec::new();
            for (i, (at, v, ease)) in ordered.iter().enumerate() {
                if i == 0 || i == ordered.len() - 1 {
                    kept.push((*at, *v, *ease));
                    continue;
                }
                let f = if span <= 0.0 {
                    0.0
                } else {
                    (at - first_at) / span
                };
                let predicted = first_v + f * (last_v - first_v);
                if (v - predicted).abs() > *tolerance {
                    kept.push((*at, *v, *ease));
                }
            }
            if kept.len() == ordered.len() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: every stop across {start}..{end} deviates past {tolerance} of straight; simplify keeps every stop, so nothing stages. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            Ok(kept
                .into_iter()
                .map(|(at, value, ease)| stop(at, value, ease))
                .collect())
        }
    }
}

/// Build the fresh key of one lane paste (Sprint 18, T007): at least
/// two stops in strictly rising time order with values in the
/// property range and parsed easings, before the caller mutates.
/// Check one composition-scoped op against the open composition
/// (Sprint 18, T008): clipboard pastes and cuts name their timeline,
/// so applying one while another timeline is open refuses instead of
/// landing clips in the wrong film. Delta replay consults
/// [`op_composition_scope`] first and never reaches this refusal.
fn check_op_scope(comp: &Composition, scope: &str, what: &str) -> Result<(), EditRefusal> {
    if comp.name == scope || comp.id.as_deref() == Some(scope) {
        return Ok(());
    }
    Err(refused(
        EditRefusalKind::BadValue,
        E_SDK_BAD_VALUE,
        format!(
            "sdk refused: {what} targets composition {scope:?} but the open composition is {:?}; open the target timeline first. Nothing was applied, previewed, or recorded.",
            comp.name
        ),
    ))
}

/// Clear a relative `start-after` link on one staged clip (Sprint
/// 18, T008): a clipboard paste lands at absolute instants, so the
/// copied resolution stays while the stale relative link drops.
/// Every other field rides along untouched.
pub(crate) fn clip_clear_relative(clip: &mut Clip) {
    match clip {
        Clip::Text(c) => c.start_after = None,
        Clip::Video(c) => c.start_after = None,
        Clip::Box(c) => c.start_after = None,
        Clip::Flex(c) => c.start_after = None,
        Clip::Image(c) => c.start_after = None,
        Clip::Path(c) => c.start_after = None,
        Clip::Svg(c) => c.start_after = None,
        Clip::Chart(c) => c.start_after = None,
        Clip::Map(c) => c.start_after = None,
        Clip::Scene3d(c) => c.start_after = None,
        Clip::Caption(c) => c.start_after = None,
        Clip::Lottie(c) => c.start_after = None,
        Clip::Audio(c) => c.start_after = None,
        Clip::Nest(c) => c.start_after = None,
    }
}

/// One landed clipboard paste (Sprint 18, T008): the clips with
/// absolute windows, renamed ids, and cleared relative links, plus
/// the motion remapped onto the landed document indices with its
/// windows shifted by the anchor delta. Pure validation and
/// transform — the caller snapshots the inverse first, then extends
/// the composition — so every refusal below stages nothing.
#[allow(clippy::too_many_arguments)]
fn land_paste_clips(
    staged: &[Clip],
    clips: &[Clip],
    tweens: &[PastedTween],
    keys: &[PastedKey],
    sets: &[PastedSet],
    at: f64,
) -> Result<(Vec<Clip>, Vec<TweenDecl>, Vec<KeyDecl>, Vec<SetDecl>), EditRefusal> {
    if clips.is_empty() {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            "sdk refused: the paste names no clips; copy at least one clip before pasting. Nothing was applied, previewed, or recorded.".to_string(),
        ));
    }
    if !at.is_finite() || at < 0.0 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the paste lands at {at:?}; write a finite film instant at or after 0. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    for (i, clip) in clips.iter().enumerate() {
        if !clip.start().is_finite() || !clip.duration().is_finite() || clip.duration() <= 0.0 {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted clip {i} carries window {:?}..{:?}; write a finite start with a finite positive duration. Nothing was applied, previewed, or recorded.",
                    clip.start(),
                    clip.start() + clip.duration()
                ),
            ));
        }
    }
    for (i, pasted) in tweens.iter().enumerate() {
        if pasted.clip >= clips.len() {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted tween {i} targets pasted clip {}; the paste carries {} clip(s). Nothing was applied, previewed, or recorded.",
                    pasted.clip,
                    clips.len()
                ),
            ));
        }
        if !pasted.tween.start.is_finite()
            || !pasted.tween.duration.is_finite()
            || pasted.tween.duration <= 0.0
        {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted tween {i} carries window {:?}..{:?}; write a finite start with a finite positive duration. Nothing was applied, previewed, or recorded.",
                    pasted.tween.start,
                    pasted.tween.start + pasted.tween.duration
                ),
            ));
        }
    }
    for (i, pasted) in keys.iter().enumerate() {
        if pasted.clip >= clips.len() {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted key {i} targets pasted clip {}; the paste carries {} clip(s). Nothing was applied, previewed, or recorded.",
                    pasted.clip,
                    clips.len()
                ),
            ));
        }
        for (s, stop) in pasted.key.stops.iter().enumerate() {
            if !stop.at.is_finite() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: pasted key {i} stop {s} sits at {:?}; write a finite instant. Nothing was applied, previewed, or recorded.",
                        stop.at
                    ),
                ));
            }
        }
    }
    for (i, pasted) in sets.iter().enumerate() {
        if pasted.clip >= clips.len() {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted set {i} targets pasted clip {}; the paste carries {} clip(s). Nothing was applied, previewed, or recorded.",
                    pasted.clip,
                    clips.len()
                ),
            ));
        }
        if !pasted.set.start.is_finite()
            || !pasted.set.duration.is_finite()
            || pasted.set.duration <= 0.0
        {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted set {i} carries window {:?}..{:?}; write a finite start with a finite positive duration. Nothing was applied, previewed, or recorded.",
                    pasted.set.start,
                    pasted.set.start + pasted.set.duration
                ),
            ));
        }
    }
    let base = clips.iter().map(Clip::start).fold(f64::INFINITY, f64::min);
    let shift = at - base;
    let mut taken: Vec<String> = staged
        .iter()
        .filter_map(|c| c.id().map(str::to_string))
        .collect();
    let mut landed = Vec::with_capacity(clips.len());
    for clip in clips {
        let mut one = clip.clone();
        let start = one.start() + shift;
        if !start.is_finite() {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: the paste lands pasted clip {:?} at {start:?}; the anchor shift runs out of range. Nothing was applied, previewed, or recorded.",
                    one.id()
                ),
            ));
        }
        one.set_start(start);
        clip_clear_relative(&mut one);
        if let Some(id) = one.id().map(str::to_string) {
            let mut candidate = format!("{id}-paste");
            let mut n = 2u32;
            while taken.iter().any(|t| t == &candidate) {
                candidate = format!("{id}-paste-{n}");
                n += 1;
            }
            taken.push(candidate.clone());
            clip_set_id(&mut one, Some(candidate));
        }
        landed.push(one);
    }
    let first = staged.len();
    let mut landed_tweens = Vec::with_capacity(tweens.len());
    for pasted in tweens {
        let mut tween = pasted.tween.clone();
        tween.target = first + pasted.clip;
        tween.start += shift;
        landed_tweens.push(tween);
    }
    let mut landed_keys = Vec::with_capacity(keys.len());
    for pasted in keys {
        let mut key = pasted.key.clone();
        key.target = first + pasted.clip;
        for stop in &mut key.stops {
            stop.at += shift;
        }
        landed_keys.push(key);
    }
    let mut landed_sets = Vec::with_capacity(sets.len());
    for pasted in sets {
        let mut set = pasted.set.clone();
        set.target = first + pasted.clip;
        set.start += shift;
        landed_sets.push(set);
    }
    Ok((landed, landed_tweens, landed_keys, landed_sets))
}

fn build_lane_paste(
    target: usize,
    property: &str,
    stops_raw: &[KeyStopInput],
) -> Result<KeyDecl, EditRefusal> {
    if stops_raw.len() < 2 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "sdk refused: the paste carries {} stop(s); write at least two stops in rising time order so the pasted key can stand. Nothing was applied, previewed, or recorded.",
                stops_raw.len()
            ),
        ));
    }
    let mut stops = Vec::with_capacity(stops_raw.len());
    let mut prev_at: Option<f64> = None;
    for (i, raw) in stops_raw.iter().enumerate() {
        if !raw.at.is_finite() || raw.at < 0.0 {
            return Err(refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted stop {i} sits at {:?}; write a finite instant at or after 0. Nothing was applied, previewed, or recorded.",
                    raw.at
                ),
            ));
        }
        if let Some(prev) = prev_at {
            if raw.at <= prev {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: pasted stop {i} sits at {} which never rises past stop {} at {prev}; write stops in strictly rising time order. Nothing was applied, previewed, or recorded.",
                        raw.at,
                        i - 1
                    ),
                ));
            }
        }
        prev_at = Some(raw.at);
        let value = parse_lane_value(&raw.value, "value", property).map_err(|_| {
            refused(
                EditRefusalKind::BadValue,
                E_SDK_BAD_VALUE,
                format!(
                    "sdk refused: pasted stop {i} value {:?} breaks the {property:?} range; write a value in range. Nothing was applied, previewed, or recorded.",
                    raw.value
                ),
            )
        })?;
        let ease = raw
            .ease
            .as_deref()
            .map(parse_ease)
            .transpose()
            .map_err(|why| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: pasted stop {i} {why}. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
        stops.push(KeyStop {
            at: raw.at,
            value,
            value_color: None,
            path: None,
            ease,
            line: 0,
            column: 0,
        });
    }
    Ok(KeyDecl {
        target,
        property: property.to_string(),
        ease: None,
        stops,
        stagger: 0.0,
        stagger_from: notation::model::StaggerOrigin::Start,
        stagger_grid: None,
        repeat: 1,
        yoyo: false,
        repeat_delay: 0.0,
        orient: false,
        line: 0,
        column: 0,
    })
}

/// Trim the covered stops of one lane (Sprint 18, T007): drop every
/// stop of the target's `property` keys inside `[start, end]`, then
/// drop the keys left empty. Stops outside the window keep their
/// exact instant, value, and easing; a key left with a single
/// outside stop gains the window-edge stop carrying the pre-trim
/// lane value (`edge_start`/`edge_end`, read before trimming), so
/// the surviving key still stands and still renders its side exactly
/// like before. The caller appends the replacement key after
/// trimming so it wins overlaps in document order.
fn trim_lane_window(
    comp: &mut Composition,
    target: usize,
    property: &str,
    start: f64,
    end: f64,
    edge_start: f64,
    edge_end: f64,
) {
    let edge = |at: f64, value: f64| KeyStop {
        at,
        value,
        value_color: None,
        path: None,
        ease: None,
        line: 0,
        column: 0,
    };
    for key in comp
        .keys
        .iter_mut()
        .filter(|k| k.target == target && k.property == property)
    {
        key.stops.retain(|s| s.at < start || s.at > end);
        if key.stops.len() == 1 {
            if key.stops[0].at < start {
                key.stops.push(edge(start, edge_start));
            } else {
                key.stops.insert(0, edge(end, edge_end));
            }
        }
    }
    comp.keys
        .retain(|k| !(k.target == target && k.property == property && k.stops.is_empty()));
}

/// Apply one forward operation to the staged composition, returning
/// the inverse restoring the exact prior state. Every check runs
/// before the mutation, so a refusal leaves the composition
/// untouched.
pub(crate) fn apply_forward(comp: &mut Composition, op: &EditOp) -> Result<RevertOp, EditRefusal> {
    match op {
        EditOp::AddElement { element, position } => {
            let clip = build_new_clip(&comp.clips, element)?;
            let at = match position {
                Some(p) => {
                    if *p > comp.clips.len() {
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "sdk refused: position {p} is past the open composition holding {} clip(s); write an index from 0 to {}. Nothing was applied, previewed, or recorded.",
                                comp.clips.len(),
                                comp.clips.len()
                            ),
                        ));
                    }
                    *p
                }
                None => comp.clips.len(),
            };
            comp.clips.insert(at, clip);
            Ok(RevertOp::RemoveClipAt { index: at })
        }
        EditOp::RemoveElement { target } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let clip = comp.clips.remove(idx);
            Ok(RevertOp::InsertClip {
                index: idx,
                clip: Box::new(clip),
            })
        }
        EditOp::MoveElement { target, to } => {
            let from = resolve_clip(&comp.clips, target)?;
            if *to >= comp.clips.len() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: move target index {to} is past the open composition holding {} clip(s); write an index from 0 to {}. Nothing was applied, previewed, or recorded.",
                        comp.clips.len(),
                        comp.clips.len().saturating_sub(1)
                    ),
                ));
            }
            // Motion targets clips by document index: shuffling the
            // range between the old and new index re-points every
            // directive inside it at another clip (Sprint 18, T004 /
            // CAP-004), so the move refuses naming the blockers instead
            // of detaching motion. Reordering with the `order` row
            // (`z-index`) keeps document indices and never trips this.
            let (lo, hi) = (from.min(*to), from.max(*to));
            let blockers = motion_targets_in(comp, lo, hi);
            if !blockers.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: moving {target} from index {from} to index {to} shuffles document indices {lo}..={hi}, re-pointing {}; motion targets clips by document index, so the move refuses instead of detaching motion. Reorder with the `order` row (`z-index`) or retarget the motion first. Nothing was applied, previewed, or recorded.",
                        blockers.join(", ")
                    ),
                ));
            }
            let back = ClipRef {
                id: comp.clips[from].id().map(|s| s.to_string()),
                index: Some(*to),
            };
            let clip = comp.clips.remove(from);
            comp.clips.insert(*to, clip);
            Ok(RevertOp::MoveClip {
                target: back,
                to: from,
            })
        }
        EditOp::AddTween {
            target,
            property,
            kind,
            from,
            to,
            start,
            duration,
            ease,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if is_audio_clip(&comp.clips[idx]) {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is an audio track; visual tweens run on picture clips, and sound envelopes arrive in a later slice. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let property = canon_property(property)?;
            let tween = build_tween(idx, property, kind, from, to, *start, *duration, ease)?;
            comp.tweens.push(tween);
            Ok(RevertOp::RemoveTween {
                index: comp.tweens.len() - 1,
            })
        }
        EditOp::RemoveTween { index } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            let tween = comp.tweens.remove(*index);
            Ok(RevertOp::InsertTween {
                index: *index,
                tween: Box::new(tween),
            })
        }
        EditOp::AddKey {
            target,
            property,
            stops,
            ease,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if is_audio_clip(&comp.clips[idx]) {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is an audio track; visual keys run on picture clips, and sound envelopes arrive in a later slice. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let property = canon_property(property)?;
            let key = build_key(idx, property, stops, ease)?;
            comp.keys.push(key);
            Ok(RevertOp::RemoveKey {
                index: comp.keys.len() - 1,
            })
        }
        EditOp::RemoveKey { index } => {
            if *index >= comp.keys.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: key index {index} names nothing; the open composition holds {} key(s). Nothing was applied, previewed, or recorded.",
                        comp.keys.len()
                    ),
                ));
            }
            let key = comp.keys.remove(*index);
            Ok(RevertOp::InsertKey {
                index: *index,
                key: Box::new(key),
            })
        }
        EditOp::AddSet {
            target,
            property,
            value,
            start,
            duration,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if is_audio_clip(&comp.clips[idx]) {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is an audio track; `set` holds a picture value, and sound envelopes arrive in a later slice. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let property = canon_property(property)?;
            let set = build_set(idx, property, value, *start, *duration)?;
            comp.sets.push(set);
            Ok(RevertOp::RemoveSet {
                index: comp.sets.len() - 1,
            })
        }
        EditOp::RemoveSet { index } => {
            if *index >= comp.sets.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: set index {index} names nothing; the open composition holds {} set(s). Nothing was applied, previewed, or recorded.",
                        comp.sets.len()
                    ),
                ));
            }
            let set = comp.sets.remove(*index);
            Ok(RevertOp::InsertSet {
                index: *index,
                set: Box::new(set),
            })
        }
        EditOp::AddPathTween {
            target,
            path,
            path_to,
            orient,
            start,
            duration,
            ease,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if is_audio_clip(&comp.clips[idx]) {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is an audio track; path travel moves picture clips, and sound envelopes arrive in a later slice. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let ease = parse_ease(ease).map_err(|why| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!("sdk refused: {why}. Nothing was applied, previewed, or recorded."),
                )
            })?;
            check_motion_window(*start, *duration, "path tween")?;
            let path = check_path_data(path, "path")?;
            let path_to = match path_to {
                Some(raw) => Some(check_path_data(raw, "path-to")?),
                None => None,
            };
            check_path_morph(&path, path_to.as_deref())?;
            comp.tweens.push(TweenDecl {
                target: idx,
                property: String::new(),
                kind: TweenKind::To,
                from: None,
                to: None,
                from_color: None,
                to_color: None,
                start: *start,
                duration: *duration,
                ease,
                stagger: 0.0,
                stagger_from: notation::model::StaggerOrigin::Start,
                stagger_grid: None,
                repeat: 1,
                yoyo: false,
                repeat_delay: 0.0,
                path: Some(path),
                path_to,
                orient: *orient,
                expr: None,
                line: 0,
                column: 0,
            });
            Ok(RevertOp::RemoveTween {
                index: comp.tweens.len() - 1,
            })
        }
        EditOp::SetTweenPath {
            index,
            path,
            path_to,
        } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            if comp.tweens[*index].path.is_none() {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: tween {index} travels no path; retargeting edits path tweens only — add a path tween or edit the numeric endpoints instead. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let path = check_path_data(path, "path")?;
            let path_to = match path_to {
                Some(raw) => Some(check_path_data(raw, "path-to")?),
                None => None,
            };
            check_path_morph(&path, path_to.as_deref())?;
            let slot = &mut comp.tweens[*index];
            let old_path = slot.path.replace(path);
            let old_to = std::mem::replace(&mut slot.path_to, path_to);
            Ok(RevertOp::RestoreTweenPath {
                index: *index,
                path: old_path,
                path_to: old_to,
            })
        }
        EditOp::SetTweenEase { index, ease } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            let decl = parse_ease(ease).map_err(|why| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: tween {index} easing {ease:?} {why}. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
            let old = std::mem::replace(&mut comp.tweens[*index].ease, decl);
            Ok(RevertOp::RestoreTweenEase {
                index: *index,
                ease: old.name(),
            })
        }
        EditOp::RetimeKeyStop { key, stop, at } => {
            let entry = key_stop_mut(comp, *key, *stop)?;
            if !at.is_finite() || *at < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key {key} stop {stop} retimes to {at:?}; write a finite instant at or after 0. Nothing was applied, previewed, or recorded.",
                    ),
                ));
            }
            if *stop > 0 && *at <= entry.prev_at {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key {key} stop {stop} retimes to {at} which never rises past stop {} at {}; write an instant past the previous stop. Nothing was applied, previewed, or recorded.",
                        stop - 1,
                        entry.prev_at
                    ),
                ));
            }
            if *stop + 1 < entry.stop_count && *at >= entry.next_at {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key {key} stop {stop} retimes to {at} which never stays under stop {} at {}; write an instant before the next stop. Nothing was applied, previewed, or recorded.",
                        stop + 1,
                        entry.next_at
                    ),
                ));
            }
            let old = std::mem::replace(&mut entry.slot.at, *at);
            Ok(RevertOp::RestoreKeyStopAt {
                key: *key,
                stop: *stop,
                at: old,
            })
        }
        EditOp::SetKeyEase { key, stop, ease } => {
            let decl = parse_ease(ease).map_err(|why| {
                refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: key {key} easing {ease:?} {why}. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
            let slot = key_ease_slot(comp, *key, *stop)?;
            let old = slot.replace(decl);
            Ok(RevertOp::RestoreKeyEase {
                key: *key,
                stop: *stop,
                ease: old.map(|e| e.name()),
            })
        }
        EditOp::RemoveKeyStop { key, stop } => {
            let (index, snapshot) = key_stop_snapshot(comp, *key, *stop)?;
            if comp.keys[index].stops.len() <= 2 {
                let removed = comp.keys.remove(index);
                Ok(RevertOp::InsertKey {
                    index,
                    key: Box::new(removed),
                })
            } else {
                comp.keys[index].stops.remove(*stop);
                Ok(RevertOp::InsertKeyStop {
                    key: index,
                    stop: *stop,
                    at: snapshot.at,
                    value: snapshot.value,
                    value_color: snapshot.value_color,
                    ease: snapshot.ease.map(|e| e.name()),
                })
            }
        }
        EditOp::DuplicateKey { key } => {
            if *key >= comp.keys.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: key index {key} names nothing; the open composition holds {} key(s). Nothing was applied, previewed, or recorded.",
                        comp.keys.len()
                    ),
                ));
            }
            comp.keys.push(comp.keys[*key].clone());
            Ok(RevertOp::RemoveKey {
                index: comp.keys.len() - 1,
            })
        }
        EditOp::RewriteLane {
            target,
            property,
            start,
            end,
            edit,
        } => {
            let idx = resolve_lane_clip(&comp.clips, target)?;
            let property = canon_lane_property(property)?;
            check_lane_window(*start, *end)?;
            let in_window = lane_stops_in(comp, idx, &property, *start, *end);
            if in_window.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: the lane `{property}` on {target} holds no stops across {start}..{end}; the range is empty, so nothing rewrites. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let fresh = build_lane_rewrite(comp, idx, &property, *start, *end, edit, &in_window)?;
            let edge_start = lane_value_at(comp, idx, &property, *start);
            let edge_end = lane_value_at(comp, idx, &property, *end);
            let snapshot = comp.keys.clone();
            trim_lane_window(comp, idx, &property, *start, *end, edge_start, edge_end);
            comp.keys.push(KeyDecl {
                target: idx,
                property,
                ease: None,
                stops: fresh,
                stagger: 0.0,
                stagger_from: notation::model::StaggerOrigin::Start,
                stagger_grid: None,
                repeat: 1,
                yoyo: false,
                repeat_delay: 0.0,
                orient: false,
                line: 0,
                column: 0,
            });
            Ok(RevertOp::RestoreKeys { keys: snapshot })
        }
        EditOp::PasteLane {
            target,
            property,
            stops,
        } => {
            let idx = resolve_lane_clip(&comp.clips, target)?;
            let property = canon_lane_property(property)?;
            let fresh = build_lane_paste(idx, &property, stops)?;
            let (first, last) = (fresh.stops[0].at, fresh.stops[fresh.stops.len() - 1].at);
            let edge_start = lane_value_at(comp, idx, &property, first);
            let edge_end = lane_value_at(comp, idx, &property, last);
            let snapshot = comp.keys.clone();
            trim_lane_window(comp, idx, &property, first, last, edge_start, edge_end);
            comp.keys.push(fresh);
            Ok(RevertOp::RestoreKeys { keys: snapshot })
        }
        EditOp::PasteClips {
            composition,
            clips,
            tweens,
            keys,
            sets,
            at,
        } => {
            check_op_scope(comp, composition, "the paste")?;
            let (landed, landed_tweens, landed_keys, landed_sets) =
                land_paste_clips(&comp.clips, clips, tweens, keys, sets, *at)?;
            let inverse = RevertOp::RestorePasted {
                clips: comp.clips.clone(),
                tweens: comp.tweens.clone(),
                keys: comp.keys.clone(),
                sets: comp.sets.clone(),
            };
            comp.clips.extend(landed);
            comp.tweens.extend(landed_tweens);
            comp.keys.extend(landed_keys);
            comp.sets.extend(landed_sets);
            Ok(inverse)
        }
        EditOp::RemoveElements {
            composition,
            targets,
        } => {
            check_op_scope(comp, composition, "the removal")?;
            if targets.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    "sdk refused: the removal names no clips; pass at least one clip `id` or index. Nothing was applied, previewed, or recorded.".to_string(),
                ));
            }
            let mut indices = Vec::with_capacity(targets.len());
            for target in targets {
                let idx = resolve_clip(&comp.clips, target)?;
                if !indices.contains(&idx) {
                    indices.push(idx);
                }
            }
            indices.sort_unstable_by(|a, b| b.cmp(a));
            let inverse = RevertOp::RestoreClips {
                clips: comp.clips.clone(),
            };
            for idx in indices {
                comp.clips.remove(idx);
            }
            Ok(inverse)
        }
        EditOp::SetLabel { target, label } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let old = std::mem::replace(clip_label_mut(&mut comp.clips[idx]), label.clone());
            Ok(RevertOp::RestoreLabel {
                target: ClipRef {
                    id: comp.clips[idx].id().map(|s| s.to_string()),
                    index: Some(idx),
                },
                label: old,
            })
        }
        EditOp::AppendArc {
            target,
            rx,
            ry,
            rot,
            large,
            sweep,
            x,
            y,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let old = clip_path_d(&comp.clips[idx]).ok_or_else(|| {
                refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is a {}; arc segments append to path clips only. Nothing was applied, previewed, or recorded.",
                        comp.clips[idx].what()
                    ),
                )
            })?;
            for (key, v) in [
                ("rx", *rx),
                ("ry", *ry),
                ("rot", *rot),
                ("x", *x),
                ("y", *y),
            ] {
                if !v.is_finite() {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: arc `{key}` value {v:?} is not finite; write a plain number. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            if *rx < 0.0 || *ry < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: arc radii ({rx},{ry}) run negative; write non-negative radii. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let old = old.to_string();
            let new_d = format!(
                "{old} A {} {} {} {} {} {} {}",
                rx,
                ry,
                rot,
                if *large { 1 } else { 0 },
                if *sweep { 1 } else { 0 },
                x,
                y
            );
            // The old data parsed at validation and the appended arc
            // is well-formed, so this re-parse only guards the join.
            if motion::path::parse(&new_d).is_err() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: appending the arc to {target} shapes no parseable path; check the radii and endpoint. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            *clip_path_d_mut(&mut comp.clips[idx]).expect("path clip") = new_d;
            Ok(RevertOp::RestorePathD {
                target: ClipRef {
                    id: comp.clips[idx].id().map(|s| s.to_string()),
                    index: Some(idx),
                },
                d: old,
            })
        }
        EditOp::Retiming {
            target,
            start,
            duration,
            trim_start,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let old_start = comp.clips[idx].start();
            let old_duration = comp.clips[idx].duration();
            let old_trim = clip_media_window(&comp.clips[idx]).map(|w| w.0);
            let new_start = start.unwrap_or(old_start);
            let new_duration = duration.unwrap_or(old_duration);
            check_window(new_start, new_duration, "retimed clip")?;
            if let Some(t) = trim_start {
                if !t.is_finite() || *t < 0.0 {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: retime of {target} carries `trim-start` {t:?}; write finite source seconds from 0. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
                if clip_media_window(&comp.clips[idx]).is_none() {
                    return Err(refused(
                        EditRefusalKind::Unsupported,
                        E_SDK_UNSUPPORTED,
                        format!(
                            "sdk refused: {target} is a {}; `trim-start` opens a file window on `video`, `audio`, and `nest` clips only. Nothing was applied, previewed, or recorded.",
                            comp.clips[idx].what()
                        ),
                    ));
                }
            }
            comp.clips[idx].set_start(new_start);
            comp.clips[idx].set_duration(new_duration);
            if let Some(t) = trim_start {
                let (slot, _) = clip_media_window_mut(&mut comp.clips[idx])
                    .expect("footage clip carries a file window");
                *slot = *t;
            }
            Ok(RevertOp::RestoreTiming {
                target: ClipRef {
                    id: comp.clips[idx].id().map(|s| s.to_string()),
                    index: Some(idx),
                },
                start: old_start,
                duration: old_duration,
                trim_start: old_trim,
            })
        }
        EditOp::SplitElement { target, at } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if matches!(comp.clips[idx], Clip::Flex(_)) {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: {target} is a flex container; splitting a laid-out subtree is outside this slice — split one of its items instead. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            if !at.is_finite() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: split of {target} at {at:?} is not finite; write a finite film time. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let original = comp.clips[idx].clone();
            let start = original.start();
            let end = start + original.duration();
            if !(*at > start && *at < end) {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: split of {target} at {at} sits outside its window {start}..{end}; write a film time strictly inside the clip. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let mut first = original.clone();
            let mut second = original.clone();
            // The first half anchors the stable `id`; the second half
            // drops it so `id` values stay unique.
            clip_set_id(&mut second, None);
            first.set_duration(*at - start);
            second.set_start(*at);
            second.set_duration(end - *at);
            if let Some((old_trim, old_end, rate, looping)) = clip_media_window(&original) {
                if !looping {
                    // Divide the file window so both halves show the
                    // same frames they showed before the split.
                    let mid = old_trim + (*at - start) * rate;
                    let first_end = match old_end {
                        Some(t) => Some(t.min(mid)),
                        None => Some(mid),
                    };
                    let (first_trim_end, second_trim_start) = (
                        clip_media_window_mut(&mut first)
                            .expect("footage half carries a file window"),
                        clip_media_window_mut(&mut second)
                            .expect("footage half carries a file window"),
                    );
                    *first_trim_end.1 = first_end;
                    *second_trim_start.0 = mid;
                }
            }
            comp.clips[idx] = first;
            comp.clips.insert(idx + 1, second);
            Ok(RevertOp::RestoreSplit {
                index: idx,
                clip: Box::new(original),
            })
        }
        EditOp::DuplicateElement { target, at, new_id } => {
            let idx = resolve_clip(&comp.clips, target)?;
            if let Some(want) = new_id {
                check_new_id(&comp.clips, &Some(want.clone()), "duplicated element")?;
            }
            let mut copy = comp.clips[idx].clone();
            clip_set_id(&mut copy, new_id.clone());
            let place = at.unwrap_or_else(|| copy.start() + copy.duration());
            if !place.is_finite() || place < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: duplicate of {target} at {place:?} is not a film time at or after 0; write finite seconds from 0. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            copy.set_start(place);
            comp.clips.insert(idx + 1, copy);
            Ok(RevertOp::RemoveClipAt { index: idx + 1 })
        }
        EditOp::SetHidden { target, hidden } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let old = comp.clips[idx].hidden();
            clip_set_hidden(&mut comp.clips[idx], *hidden);
            Ok(RevertOp::RestoreHidden {
                target: ClipRef {
                    id: comp.clips[idx].id().map(|s| s.to_string()),
                    index: Some(idx),
                },
                hidden: old,
            })
        }
        EditOp::GroupElements { targets, name } => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    "sdk refused: grouping needs a non-empty group name; pass `\"name\": \"credits\"` or another name. Nothing was applied, previewed, or recorded.".to_string(),
                ));
            }
            if targets.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: group {name:?} names no clips; pass at least one clip `id` or index. Grouping an empty selection does nothing. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let mut indices = Vec::new();
            for target in targets {
                let idx = resolve_clip(&comp.clips, target)?;
                if !indices.contains(&idx) {
                    indices.push(idx);
                }
            }
            let note = group_label_for(&name);
            for &idx in &indices {
                match comp.clips[idx].label() {
                    Some(current) if current == note => {
                        let key = comp.clips[idx]
                            .id()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("#{idx}"));
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "sdk refused: {key:?} is already grouped in {name:?}; ungroup it first or group into another name. Nothing was applied, previewed, or recorded."
                            ),
                        ));
                    }
                    Some(current) if group_name_of_label(current).is_some() => {
                        let key = comp.clips[idx]
                            .id()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("#{idx}"));
                        let other = group_name_of_label(current).expect("grouped note");
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "sdk refused: {key:?} is already grouped in {other:?}; ungroup it from there first. Nothing was applied, previewed, or recorded."
                            ),
                        ));
                    }
                    Some(_) => {
                        let key = comp.clips[idx]
                            .id()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("#{idx}"));
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "sdk refused: {key:?} carries an editor note, which grouping would overwrite; clear or rename the note first. Nothing was applied, previewed, or recorded."
                            ),
                        ));
                    }
                    None => {}
                }
            }
            let mut members = Vec::new();
            for &idx in &indices {
                let old =
                    std::mem::replace(clip_label_mut(&mut comp.clips[idx]), Some(note.clone()));
                members.push(GroupLabelRestore {
                    target: ClipRef {
                        id: comp.clips[idx].id().map(|s| s.to_string()),
                        index: Some(idx),
                    },
                    label: old,
                });
            }
            Ok(RevertOp::RestoreGroupLabels { members })
        }
        EditOp::UngroupElements { name } => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    "sdk refused: ungrouping needs the grouped name; pass the `\"name\"` the group was collected under. Nothing was applied, previewed, or recorded.".to_string(),
                ));
            }
            let note = group_label_for(&name);
            let mut indices = Vec::new();
            for (idx, clip) in comp.clips.iter().enumerate() {
                if clip.label() == Some(note.as_str()) {
                    indices.push(idx);
                }
            }
            if indices.is_empty() {
                let holdings = staged_group_names(comp);
                let holdings = if holdings.is_empty() {
                    "the open composition holds no layer groups".to_string()
                } else {
                    format!(
                        "the open composition groups {}",
                        holdings
                            .iter()
                            .map(|n| format!("{n:?}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: ungroup {name:?} names no grouped members; {holdings}. Pass a grouped name. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            let mut members = Vec::new();
            for &idx in &indices {
                let old = std::mem::replace(clip_label_mut(&mut comp.clips[idx]), None);
                members.push(GroupLabelRestore {
                    target: ClipRef {
                        id: comp.clips[idx].id().map(|s| s.to_string()),
                        index: Some(idx),
                    },
                    label: old,
                });
            }
            Ok(RevertOp::RestoreGroupLabels { members })
        }
        EditOp::SetRow {
            target,
            property,
            value,
        } => {
            let property = crate::rows::canonical_row_property(property);
            // Render-deferred rows refuse before anything resolves,
            // naming their milestone so the inspector marks them
            // unavailable instead of staging them.
            if let crate::rows::RowAvailability::Unavailable { milestone } =
                crate::rows::row_availability(&property)
            {
                return Err(refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: row `{property}` waits for the {milestone}; rows cover the render-supported 2D property set today. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            if !crate::rows::ACCEPTED_ROWS.contains(&property.as_str()) {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: row key {property:?} names nothing settable; write one of {}. Nothing was applied, previewed, or recorded.",
                        crate::rows::ACCEPTED_ROWS.join(", ")
                    ),
                ));
            }
            let idx = resolve_clip(&comp.clips, target)?;
            let prior =
                crate::rows::apply_row_value(idx, &mut comp.clips[idx], &property, value, target)?;
            Ok(RevertOp::RestoreRow {
                target: ClipRef {
                    id: comp.clips[idx].id().map(|s| s.to_string()),
                    index: Some(idx),
                },
                property,
                prior,
            })
        }
    }
}

/// Apply one inverse to the staged composition, restoring the exact
/// prior state. Used by tests and by undo/redo walking (T003).
pub(crate) fn apply_revert(comp: &mut Composition, revert: &RevertOp) -> Result<(), EditRefusal> {
    match revert {
        RevertOp::RemoveClipAt { index } => {
            if *index >= comp.clips.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target clip index {index} is past the open composition holding {} clip(s). Nothing was applied, previewed, or recorded.",
                        comp.clips.len()
                    ),
                ));
            }
            comp.clips.remove(*index);
            Ok(())
        }
        RevertOp::InsertClip { index, clip } => {
            if *index > comp.clips.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target clip index {index} is past the open composition holding {} clip(s). Nothing was applied, previewed, or recorded.",
                        comp.clips.len()
                    ),
                ));
            }
            if let Some(id) = clip.id() {
                if comp.clips.iter().any(|c| c.id() == Some(id)) {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: revert restores `id` {id:?} but the open composition already takes it; `id` values stay unique. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            comp.clips.insert(*index, clip.as_ref().clone());
            Ok(())
        }
        RevertOp::MoveClip { target, to } => {
            let from = resolve_clip(&comp.clips, target)?;
            if *to >= comp.clips.len() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "sdk refused: revert target index {to} is past the open composition holding {} clip(s). Nothing was applied, previewed, or recorded.",
                        comp.clips.len()
                    ),
                ));
            }
            let clip = comp.clips.remove(from);
            comp.clips.insert(*to, clip);
            Ok(())
        }
        RevertOp::RemoveTween { index } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            comp.tweens.remove(*index);
            Ok(())
        }
        RevertOp::InsertTween { index, tween } => {
            if *index > comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target tween index {index} is past {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            let mut tween = tween.as_ref().clone();
            tween.target = tween.target.min(comp.clips.len().saturating_sub(1));
            comp.tweens.insert(*index, tween);
            Ok(())
        }
        RevertOp::RemoveKey { index } => {
            if *index >= comp.keys.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target key index {index} names nothing; the open composition holds {} key(s). Nothing was applied, previewed, or recorded.",
                        comp.keys.len()
                    ),
                ));
            }
            comp.keys.remove(*index);
            Ok(())
        }
        RevertOp::InsertKey { index, key } => {
            if *index > comp.keys.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target key index {index} is past {} key(s). Nothing was applied, previewed, or recorded.",
                        comp.keys.len()
                    ),
                ));
            }
            let mut key = key.as_ref().clone();
            key.target = key.target.min(comp.clips.len().saturating_sub(1));
            comp.keys.insert(*index, key);
            Ok(())
        }
        RevertOp::RemoveSet { index } => {
            if *index >= comp.sets.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target set index {index} names nothing; the open composition holds {} set(s). Nothing was applied, previewed, or recorded.",
                        comp.sets.len()
                    ),
                ));
            }
            comp.sets.remove(*index);
            Ok(())
        }
        RevertOp::InsertSet { index, set } => {
            if *index > comp.sets.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target set index {index} is past {} set(s). Nothing was applied, previewed, or recorded.",
                        comp.sets.len()
                    ),
                ));
            }
            let mut set = set.as_ref().clone();
            set.target = set.target.min(comp.clips.len().saturating_sub(1));
            comp.sets.insert(*index, set);
            Ok(())
        }
        RevertOp::RestoreTweenPath {
            index,
            path,
            path_to,
        } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            let slot = &mut comp.tweens[*index];
            slot.path = path.clone();
            slot.path_to = path_to.clone();
            Ok(())
        }
        RevertOp::RestoreTweenEase { index, ease } => {
            if *index >= comp.tweens.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target tween index {index} names nothing; the open composition holds {} tween(s). Nothing was applied, previewed, or recorded.",
                        comp.tweens.len()
                    ),
                ));
            }
            let decl = parse_ease(ease).map_err(|why| {
                refused(
                    EditRefusalKind::BrokenTurn,
                    E_SDK_BROKEN_TURN,
                    format!(
                        "sdk refused: revert target tween {index} easing restores nothing runnable: {why}. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
            comp.tweens[*index].ease = decl;
            Ok(())
        }
        RevertOp::RestoreLabel { target, label } => {
            let idx = resolve_clip(&comp.clips, target)?;
            *clip_label_mut(&mut comp.clips[idx]) = label.clone();
            Ok(())
        }
        RevertOp::RestoreKeys { keys } => {
            comp.keys = keys.clone();
            Ok(())
        }
        RevertOp::RestorePasted {
            clips,
            tweens,
            keys,
            sets,
        } => {
            comp.clips = clips.clone();
            comp.tweens = tweens.clone();
            comp.keys = keys.clone();
            comp.sets = sets.clone();
            Ok(())
        }
        RevertOp::RestoreClips { clips } => {
            comp.clips = clips.clone();
            Ok(())
        }
        RevertOp::RestoreGroupLabels { members } => {
            for member in members {
                let idx = resolve_clip(&comp.clips, &member.target)?;
                *clip_label_mut(&mut comp.clips[idx]) = member.label.clone();
            }
            Ok(())
        }
        RevertOp::RestorePathD { target, d } => {
            let idx = resolve_clip(&comp.clips, target)?;
            let what = comp.clips[idx].what();
            let slot = clip_path_d_mut(&mut comp.clips[idx]).ok_or_else(|| {
                refused(
                    EditRefusalKind::Unsupported,
                    E_SDK_UNSUPPORTED,
                    format!(
                        "sdk refused: revert target {target} is a {what}; path data restores on path clips only. Nothing was applied, previewed, or recorded."
                    ),
                )
            })?;
            *slot = d.clone();
            Ok(())
        }
        RevertOp::RestoreTiming {
            target,
            start,
            duration,
            trim_start,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            comp.clips[idx].set_start(*start);
            comp.clips[idx].set_duration(*duration);
            if let Some(old) = trim_start {
                if let Some((slot, _)) = clip_media_window_mut(&mut comp.clips[idx]) {
                    *slot = *old;
                }
            }
            Ok(())
        }
        RevertOp::RestoreSplit { index, clip } => {
            if comp.clips.len() < index + 2 {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target split at index {index} names two halves but the open composition holds {} clip(s); its target is missing. Nothing was applied, previewed, or recorded.",
                        comp.clips.len()
                    ),
                ));
            }
            if let Some(id) = clip.id() {
                if comp
                    .clips
                    .iter()
                    .enumerate()
                    .any(|(i, c)| i != *index && i != index + 1 && c.id() == Some(id))
                {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: revert restores `id` {id:?} but the open composition already takes it; its backup cannot restore. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            comp.clips.remove(index + 1);
            comp.clips.remove(*index);
            comp.clips.insert(*index, clip.as_ref().clone());
            Ok(())
        }
        RevertOp::RestoreHidden { target, hidden } => {
            let idx = resolve_clip(&comp.clips, target)?;
            clip_set_hidden(&mut comp.clips[idx], *hidden);
            Ok(())
        }
        RevertOp::RestoreRow {
            target,
            property,
            prior,
        } => {
            let idx = resolve_clip(&comp.clips, target)?;
            // The prior rendered value validated when it was live, so
            // it validates now; a kind change in between (which no op
            // performs) refuses naming the rule instead of guessing.
            let _ = crate::rows::apply_row_value(idx, &mut comp.clips[idx], property, prior, target)
                .map_err(|e| {
                    refused(
                        EditRefusalKind::UnknownTarget,
                        E_SDK_UNKNOWN_TARGET,
                        format!(
                            "sdk refused: revert target row `{property}` on {target} no longer restores ({e}); its target is gone. Nothing was applied, previewed, or recorded."
                        ),
                    )
                })?;
            Ok(())
        }
        RevertOp::RestoreKeyStopAt { key, stop, at } => {
            let entry = key_stop_mut(comp, *key, *stop)?;
            if *stop > 0 && *at <= entry.prev_at {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target key {key} stop {stop} at {at} no longer rises past its neighbor; its target is gone. Nothing was applied, previewed, or recorded.",
                    ),
                ));
            }
            if *stop + 1 < entry.stop_count && *at >= entry.next_at {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target key {key} stop {stop} at {at} no longer stays under its neighbor; its target is gone. Nothing was applied, previewed, or recorded.",
                    ),
                ));
            }
            entry.slot.at = *at;
            Ok(())
        }
        RevertOp::RestoreKeyEase { key, stop, ease } => {
            let slot = key_ease_slot(comp, *key, *stop)?;
            *slot = match ease {
                None => None,
                Some(name) => Some(parse_ease(name).map_err(|why| {
                    refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: revert restores easing {name:?} but it no longer parses ({why}); its backup cannot restore. Nothing was applied, previewed, or recorded."
                        ),
                    )
                })?),
            };
            Ok(())
        }
        RevertOp::InsertKeyStop {
            key,
            stop,
            at,
            value,
            value_color,
            ease,
        } => {
            let index = key_index(comp, *key)?;
            if *stop > comp.keys[index].stops.len() {
                return Err(refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "sdk refused: revert target key {key} stop index {stop} is past {} stop(s). Nothing was applied, previewed, or recorded.",
                        comp.keys[index].stops.len()
                    ),
                ));
            }
            let parsed_ease = match ease {
                None => None,
                Some(name) => Some(parse_ease(name).map_err(|why| {
                    refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        format!(
                            "sdk refused: revert restores stop easing {name:?} but it no longer parses ({why}); its backup cannot restore. Nothing was applied, previewed, or recorded."
                        ),
                    )
                })?),
            };
            let snapshot = KeyStop {
                at: *at,
                value: *value,
                value_color: *value_color,
                path: None,
                ease: parsed_ease,
                line: 0,
                column: 0,
            };
            comp.keys[index].stops.insert(*stop, snapshot);
            Ok(())
        }
    }
}
