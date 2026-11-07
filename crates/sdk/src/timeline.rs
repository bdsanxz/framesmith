//! Timeline verbs (Sprint 15, T007 / CAP-006).
//!
//! Verbs are the human CLI surface over the same patch engine agents
//! use: `move`, `trim`, `split`, `delete`, `set`, `duplicate`, and
//! `ids` rewrite the staged composition through the patch machinery
//! from T002/T003 ([`EditOp`] applied with [`SdkSession::apply`]), so
//! every verb application stages patches with inverses and
//! [`SdkSession::timeline_undo`] restores the exact prior sources.
//! `apply` prints an undo receipt; `undo` consumes it.
//!
//! Time expressions resolve against the staged composition in four
//! forms: `+N` (N seconds after the target clip's start), `Nf` (film
//! frame N at the composition rate), `end` (the composition end), and
//! `after:<ref>` (the end of the referenced clip, where a numeric ref
//! reads as a document index and any other ref as an `id`). Anything
//! else refuses with a reason code before anything changes, and a
//! refused apply leaves no receipt.
//!
//! Plan mode ([`SdkSession::timeline_plan`]) prints only a count and
//! changes nothing; apply mode ([`SdkSession::timeline_apply`])
//! stages the patches and returns a printable undo receipt. Rows
//! ([`SdkSession::timeline_rows`]) expose `mediaStart`: the file
//! seconds mapping to the film window, i.e. `trim_start` on
//! footage-style clips (`video`, `audio`, `nest`) and 0 otherwise.
//!
//! Ranged delete (`delete` with `--start`/`--end`) removes only that
//! range: clips fully inside drop, straddling clips shorten, and a
//! clip strictly containing the range splits in two. `--snap` (a
//! tolerance in seconds) pulls a range edge onto a clip edge it nearly
//! touches, covering delete exactly like the head/tail trims.
//!
//! Persistence boundary: verb application stages in memory only, like
//! every other apply. Nothing is written to disk — not the composition
//! sources, not the hidden history directory — so plan and apply both
//! keep the project tree byte-identical until an explicit persist.
//! Style boundary: no new style attributes arrive here, so the
//! CSS-like hyphenated KDL vocabulary passes through untouched, and
//! `font-family` try-order lists are never resolved or reordered.

use notation::model::{Clip, Composition};

use crate::edits::{
    self, ClipRef, EditOp, EditRefusal, EditRefusalKind, E_SDK_BAD_VALUE, E_SDK_NOTHING_TO_UNDO,
    E_SDK_UNKNOWN_TARGET, E_SDK_UNSUPPORTED,
};
use crate::SdkSession;

fn refused(kind: EditRefusalKind, code: &'static str, message: String) -> EditRefusal {
    EditRefusal {
        kind,
        code,
        message,
    }
}

/// One film-time expression (Sprint 15, T007): `+N` seconds after the
/// target clip's start, `Nf` film frame N at the composition rate,
/// `end` the composition end, or `after:<ref>` the end of the
/// referenced clip (numeric refs read as document indexes, other refs
/// as `id` values).
#[derive(Debug, Clone)]
pub enum TimeExpr {
    /// N seconds after the target clip's start (`+N`).
    Offset(f64),
    /// Film frame N at the composition rate (`Nf`).
    Frames(u64),
    /// The composition end (`end`).
    End,
    /// The end of the referenced clip (`after:<ref>`).
    After(ClipRef),
}

/// Parse one time expression: exactly the four named forms, nothing
/// else. Anything unparsable refuses with [`E_SDK_BAD_VALUE`] naming
/// the forms, before anything changes.
pub fn parse_time_expr(raw: &str) -> Result<TimeExpr, EditRefusal> {
    let text = raw.trim();
    if text == "end" {
        return Ok(TimeExpr::End);
    }
    if let Some(rest) = text.strip_prefix('+') {
        if rest.is_empty() {
            return Err(bad_time(raw, "a `+N` offset needs seconds after the `+`"));
        }
        match rest.parse::<f64>() {
            Ok(n) if n.is_finite() => return Ok(TimeExpr::Offset(n)),
            _ => {
                return Err(bad_time(
                    raw,
                    "a `+N` offset needs finite seconds after the `+`",
                ));
            }
        }
    }
    if let Some(rest) = text.strip_suffix(['f', 'F']) {
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad_time(
                raw,
                "an `Nf` frame needs a whole frame count before the `f`",
            ));
        }
        match rest.parse::<u64>() {
            Ok(n) => return Ok(TimeExpr::Frames(n)),
            Err(_) => {
                return Err(bad_time(
                    raw,
                    "an `Nf` frame needs a whole frame count before the `f`",
                ));
            }
        }
    }
    if let Some(rest) = text.strip_prefix("after:") {
        if rest.is_empty() {
            return Err(bad_time(
                raw,
                "an `after:<ref>` needs a clip `id` or index after the `:`",
            ));
        }
        if let Ok(index) = rest.parse::<usize>() {
            return Ok(TimeExpr::After(ClipRef {
                id: None,
                index: Some(index),
            }));
        }
        return Ok(TimeExpr::After(ClipRef::by_id(rest)));
    }
    Err(bad_time(
        raw,
        "write one of `+N` seconds, `Nf` frames, `end`, or `after:<ref>`",
    ))
}

fn bad_time(raw: &str, why: &str) -> EditRefusal {
    refused(
        EditRefusalKind::BadValue,
        E_SDK_BAD_VALUE,
        format!(
            "timeline refused: time expression {raw:?} is unresolvable: {why}. Nothing was planned or applied, and no receipt was left."
        ),
    )
}

/// Resolve one time expression to film seconds: offsets read from the
/// target clip's start, frames from the composition rate, `end` from
/// the composition duration, and `after:<ref>` from the referenced
/// clip's end. A reference naming nothing refuses with
/// [`E_SDK_UNKNOWN_TARGET`], naming the lookup.
fn resolve_time(
    expr: &TimeExpr,
    comp: &Composition,
    target_start: f64,
) -> Result<f64, EditRefusal> {
    match expr {
        TimeExpr::Offset(n) => Ok(target_start + n),
        TimeExpr::Frames(n) => Ok(*n as f64 / comp.fps.value()),
        TimeExpr::End => Ok(comp.duration),
        TimeExpr::After(target) => {
            let idx = edits::resolve_clip(&comp.clips, target).map_err(|e| {
                refused(
                    EditRefusalKind::UnknownTarget,
                    E_SDK_UNKNOWN_TARGET,
                    format!(
                        "timeline refused: `after:{}` names no clip in the open composition: {e} Nothing was planned or applied, and no receipt was left.",
                        match &target.id {
                            Some(id) => id.clone(),
                            None => format!("{:?}", target.index),
                        }
                    ),
                )
            })?;
            Ok(comp.clips[idx].start() + comp.clips[idx].duration())
        }
    }
}

/// Which property a `set` verb writes (Sprint 15, T007): the editor
/// note (`label`), the visibility switch (`hidden`), or the film
/// window (`start`, `duration`). Plain names, exactly these four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetProperty {
    /// The editor note; an empty value clears it.
    Label,
    /// The visibility switch; `true` hides, `false` shows.
    Hidden,
    /// The film-window start in seconds.
    Start,
    /// The film-window length in seconds.
    Duration,
}

/// Parse one `set` property name: exactly `label`, `hidden`, `start`,
/// or `duration`. Anything else is outside verb capability and refuses
/// with [`E_SDK_UNSUPPORTED`] naming the four.
pub fn parse_set_property(raw: &str) -> Result<SetProperty, EditRefusal> {
    match raw {
        "label" => Ok(SetProperty::Label),
        "hidden" => Ok(SetProperty::Hidden),
        "start" => Ok(SetProperty::Start),
        "duration" => Ok(SetProperty::Duration),
        _ => Err(refused(
            EditRefusalKind::Unsupported,
            E_SDK_UNSUPPORTED,
            format!(
                "timeline refused: `set` property {raw:?} is outside verb capability; write one of `label`, `hidden`, `start`, `duration`. Nothing was planned or applied, and no receipt was left."
            ),
        )),
    }
}

/// One timeline verb (Sprint 15, T007): the seven content verbs. `move`
/// shifts a clip in film time keeping its file origin; `trim` rewrites
/// a clip window (advancing the file origin past trimmed footage so
/// the same tail shows); `split` divides one clip at a film time;
/// `delete` removes a whole clip or only a `--start`/`--end` range
/// across the timeline with `--snap` tolerance; `set` writes one
/// property; `duplicate` clones a clip; `ids` lists clip identifiers
/// and never mutates. `apply` and `undo` are the session methods
/// ([`SdkSession::timeline_apply`], [`SdkSession::timeline_undo`]) that
/// run a verb and reverse the last run.
#[derive(Debug, Clone)]
pub enum TimelineVerb {
    /// Shift the target clip to the resolved film time.
    Move { target: ClipRef, to: TimeExpr },
    /// Rewrite the target window to the resolved `[start, end)`; each
    /// side defaults to the current edge, and at least one side must
    /// be given.
    Trim {
        target: ClipRef,
        start: Option<TimeExpr>,
        end: Option<TimeExpr>,
    },
    /// Divide the target clip at the resolved film time.
    Split { target: ClipRef, at: TimeExpr },
    /// Remove the target clip, or — with `start`/`end` — only that
    /// film range across the timeline (or within the target when both
    /// are given). `snap` pulls a range edge onto a clip edge it
    /// touches within the tolerance, covering delete.
    Delete {
        target: Option<ClipRef>,
        start: Option<TimeExpr>,
        end: Option<TimeExpr>,
        snap: Option<f64>,
    },
    /// Write one property on the target clip.
    Set {
        target: ClipRef,
        property: SetProperty,
        value: String,
    },
    /// Clone the target clip right after itself at the resolved film
    /// time (the original end when omitted), dropping the `id` unless
    /// `new_id` names a fresh unique one.
    Duplicate {
        target: ClipRef,
        at: Option<TimeExpr>,
        new_id: Option<String>,
    },
    /// List every clip identifier; changes nothing in either mode.
    Ids,
}

/// One timeline row (Sprint 15, T007): the clip identity, lane,
/// window, and `media_start` mapping file seconds to film seconds —
/// `trim_start` on footage-style clips (`video`, `audio`, `nest`), 0
/// on every other kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRow {
    /// Document index of the clip.
    pub index: usize,
    /// Element name (`text`, `video`, `box`, ...).
    pub kind: &'static str,
    /// Stable `id`, when declared.
    pub id: Option<String>,
    /// Editor note, when declared.
    pub label: Option<String>,
    /// Display lane name.
    pub track: String,
    /// True while the clip skips picture and sound.
    pub hidden: bool,
    /// Film seconds where the window opens.
    pub start: f64,
    /// Film seconds where the window closes.
    pub end: f64,
    /// File seconds shown at the window open.
    pub media_start: f64,
}

impl std::fmt::Display for TimelineRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let id = match &self.id {
            Some(id) => format!("{id:?}"),
            None => "-".to_string(),
        };
        let label = match &self.label {
            Some(label) => format!("{label:?}"),
            None => "-".to_string(),
        };
        write!(
            f,
            "[{}] {} id={id} track={} label={label} {} window {:.3}s..{:.3}s mediaStart {:.3}s",
            self.index,
            self.kind,
            self.track,
            if self.hidden { "hidden" } else { "visible" },
            self.start,
            self.end,
            self.media_start
        )
    }
}

/// Read every timeline row of one composition, in document order.
pub fn rows_of(comp: &Composition) -> Vec<TimelineRow> {
    comp.clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let (start, end) = (clip.start(), clip.start() + clip.duration());
            TimelineRow {
                index,
                kind: clip.kind(),
                id: clip.id().map(|s| s.to_string()),
                label: clip.label().map(|s| s.to_string()),
                track: clip.track().name().to_string(),
                hidden: clip.hidden(),
                start,
                end,
                media_start: media_start_of(clip),
            }
        })
        .collect()
}

/// The file seconds shown at the clip's window open: `trim_start` on
/// footage-style clips, 0 on every other kind.
fn media_start_of(clip: &Clip) -> f64 {
    edits::clip_media_window(clip).map(|w| w.0).unwrap_or(0.0)
}

/// Human-readable timeline rows: one header plus one row per clip,
/// mirroring the `timeline` listing with the `mediaStart` column the
/// verbs report.
pub fn format_rows_human(comp: &Composition) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "timeline of composition {:?} ({} clip(s), {}x{}, {:.3}s, {}fps):",
        comp.name,
        comp.clips.len(),
        comp.width,
        comp.height,
        comp.duration,
        comp.fps
    );
    if comp.clips.is_empty() {
        let _ = writeln!(out, "  (no clips)");
    }
    for row in rows_of(comp) {
        let _ = writeln!(out, "  {row}.");
    }
    out
}

/// What [`SdkSession::timeline_plan`] reports: the verb summary, the
/// count of clips it would rewrite, and one detail line per touched
/// clip. The display prints only the count; the detail stays available
/// programmatically without printing.
#[derive(Debug, Clone)]
pub struct TimelinePlan {
    /// One-line human summary of the verb with resolved times.
    pub verb_summary: String,
    /// Clips the verb would rewrite (added halves and copies count).
    pub affected: usize,
    /// One line per touched clip, in document order.
    pub detail: Vec<String>,
}

impl std::fmt::Display for TimelinePlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "timeline plan: {} clip(s) would change", self.affected)
    }
}

/// What [`SdkSession::timeline_apply`] returns: the staged patch count
/// with the sequence numbers that undo it. The display prints the undo
/// receipt; `ids` prints the identifier listing instead (it stages no
/// patches).
#[derive(Debug, Clone)]
pub struct VerbReceipt {
    /// One-line human summary of the verb with resolved times.
    pub verb_summary: String,
    /// Patches staged by this application (0 for `ids` and for ranges
    /// touching nothing).
    pub applied: usize,
    /// Sequence numbers of the staged patches, in apply order.
    pub seqs: Vec<u64>,
    /// True when [`SdkSession::timeline_undo`] can reverse this run.
    pub undo_available: bool,
    /// Identifier lines, non-empty only for `ids`.
    pub listing: Vec<String>,
}

impl std::fmt::Display for VerbReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.listing.is_empty() || self.applied == 0 && self.verb_summary == "ids" {
            writeln!(f, "timeline ids ({} clip(s)):", self.listing.len())?;
            for line in &self.listing {
                writeln!(f, "  {line}")?;
            }
            return Ok(());
        }
        write!(
            f,
            "timeline applied {} patch(es): {}; undo receipt seqs {:?}; run `timeline undo` to restore",
            self.applied, self.verb_summary, self.seqs
        )
    }
}

/// What [`SdkSession::timeline_undo`] returns: the reversed patch
/// count with the summaries it restored.
#[derive(Debug, Clone)]
pub struct VerbUndoReceipt {
    /// Patches reversed by this undo.
    pub undone: usize,
    /// One-line summary per reversed patch, newest first.
    pub summaries: Vec<String>,
}

impl std::fmt::Display for VerbUndoReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "timeline undo: restored {} patch(es): {}; sources match the pre-verb state",
            self.undone,
            self.summaries.join("; ")
        )
    }
}

/// One expanded verb: the concrete patch operations plan and apply
/// share, with the count, summary, and per-clip detail both report.
struct Expanded {
    ops: Vec<EditOp>,
    affected: usize,
    summary: String,
    detail: Vec<String>,
}

/// Check one resolved `[start, end)` film range: finite, opening at or
/// after zero, strictly ordered. Anything else refuses with
/// [`E_SDK_BAD_VALUE`] before anything changes.
fn check_range(start: f64, end: f64, what: &str) -> Result<(), EditRefusal> {
    if !start.is_finite() || start < 0.0 || !end.is_finite() || end <= start {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "timeline refused: {what} resolves to {start}..{end}; write a finite range opening at or after 0 with the end past the start. Nothing was planned or applied, and no receipt was left."
            ),
        ));
    }
    Ok(())
}

/// The file origin showing the same tail after a head cut from
/// `from_film` to `to_film` on one clip: advanced by the cut length
/// times the rate on non-looping footage, untouched otherwise. A cut
/// reaching before file zero refuses, never wrapped silently.
fn shifted_trim(clip: &Clip, from_film: f64, to_film: f64) -> Result<Option<f64>, EditRefusal> {
    let Some((old_trim, _, rate, looping)) = edits::clip_media_window(clip) else {
        return Ok(None);
    };
    if looping {
        return Ok(None);
    }
    let shifted = old_trim + (to_film - from_film) * rate;
    if shifted < 0.0 {
        return Err(refused(
            EditRefusalKind::BadValue,
            E_SDK_BAD_VALUE,
            format!(
                "timeline refused: the cut reaches file time {shifted:.3}s, before the source opens; write a range the source covers. Nothing was planned or applied, and no receipt was left."
            ),
        ));
    }
    Ok(Some(shifted))
}

/// Expand one verb against the staged composition into concrete patch
/// operations: every time expression resolves here, every range and
/// capability checks here, so plan and apply agree exactly. Delete
/// emits operations in descending document order, so earlier inserts
/// and removals never shift a later target.
fn expand_verb(comp: &Composition, verb: &TimelineVerb) -> Result<Expanded, EditRefusal> {
    match verb {
        TimelineVerb::Move { target, to } => {
            let idx = edits::resolve_clip(&comp.clips, target)?;
            let at = resolve_time(to, comp, comp.clips[idx].start())?;
            if !at.is_finite() || at < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "timeline refused: move of {target} resolves to {at}; write a film time at or after 0. Nothing was planned or applied, and no receipt was left."
                    ),
                ));
            }
            Ok(Expanded {
                ops: vec![EditOp::Retiming {
                    target: target.clone(),
                    start: Some(at),
                    duration: None,
                    trim_start: None,
                }],
                affected: 1,
                summary: format!("move {target} to {at:.3}s"),
                detail: vec![format!("move {target} to {at:.3}s")],
            })
        }
        TimelineVerb::Trim { target, start, end } => {
            if start.is_none() && end.is_none() {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "timeline refused: trim of {target} names no edge; pass `--start` and/or `--end`. Nothing was planned or applied, and no receipt was left."
                    ),
                ));
            }
            let idx = edits::resolve_clip(&comp.clips, target)?;
            let clip = &comp.clips[idx];
            let (old_start, old_end) = (clip.start(), clip.start() + clip.duration());
            let s = match start {
                Some(e) => resolve_time(e, comp, old_start)?,
                None => old_start,
            };
            let e = match end {
                Some(e) => resolve_time(e, comp, old_start)?,
                None => old_end,
            };
            check_range(s, e, &format!("trim of {target}"))?;
            let trim_start = if s != old_start {
                shifted_trim(clip, old_start, s)?
            } else {
                None
            };
            Ok(Expanded {
                ops: vec![EditOp::Retiming {
                    target: target.clone(),
                    start: Some(s),
                    duration: Some(e - s),
                    trim_start,
                }],
                affected: 1,
                summary: format!("trim {target} to {s:.3}s..{e:.3}s"),
                detail: vec![format!("trim {target} to {s:.3}s..{e:.3}s")],
            })
        }
        TimelineVerb::Split { target, at } => {
            let idx = edits::resolve_clip(&comp.clips, target)?;
            let at = resolve_time(at, comp, comp.clips[idx].start())?;
            Ok(Expanded {
                ops: vec![EditOp::SplitElement {
                    target: target.clone(),
                    at,
                }],
                affected: 2,
                summary: format!("split {target} at {at:.3}s"),
                detail: vec![format!("split {target} at {at:.3}s")],
            })
        }
        TimelineVerb::Delete {
            target,
            start,
            end,
            snap,
        } => {
            let snap_tol = match snap {
                Some(t) => {
                    if !t.is_finite() || *t < 0.0 {
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "timeline refused: `--snap` tolerance {t:?} is not finite seconds from 0; write a tolerance at or after 0. Nothing was planned or applied, and no receipt was left."
                            ),
                        ));
                    }
                    *t
                }
                None => 0.0,
            };
            if target.is_none() && (start.is_none() || end.is_none()) {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    "timeline refused: delete names no target and no full `--start`/`--end` range; pass a clip or both range edges. Nothing was planned or applied, and no receipt was left.".to_string(),
                ));
            }
            let only = match target {
                Some(t) => Some(edits::resolve_clip(&comp.clips, t)?),
                None => None,
            };
            // Range edges resolve without a target anchor: offsets read
            // from film zero, frames and `end` are absolute, and
            // `after:<ref>` reads the referenced clip's end.
            let (rs, re) = match (start, end) {
                (Some(s), Some(e)) => {
                    let rs = resolve_time(s, comp, 0.0)?;
                    let re = resolve_time(e, comp, 0.0)?;
                    check_range(rs, re, "delete range")?;
                    (rs, re)
                }
                (None, None) => (f64::NEG_INFINITY, f64::INFINITY),
                _ => {
                    return Err(refused(
                        EditRefusalKind::BadValue,
                        E_SDK_BAD_VALUE,
                        "timeline refused: delete with a target needs both or neither range edge; pass `--start` and `--end` together or neither. Nothing was planned or applied, and no receipt was left.".to_string(),
                    ));
                }
            };
            let mut ops = Vec::new();
            let mut detail = Vec::new();
            let mut affected = 0usize;
            // Descending: removals and inserts above never shift a
            // pending lower target.
            for (i, clip) in comp.clips.iter().enumerate().rev() {
                if let Some(only_idx) = only {
                    if i != only_idx {
                        continue;
                    }
                }
                let (cs, ce) = (clip.start(), clip.start() + clip.duration());
                let s = if (rs - cs).abs() <= snap_tol { cs } else { rs };
                let e = if (re - ce).abs() <= snap_tol { ce } else { re };
                if e <= cs || s >= ce {
                    continue;
                }
                affected += 1;
                let id_ref = ClipRef {
                    id: clip.id().map(|s| s.to_string()),
                    index: Some(i),
                };
                if s <= cs && e >= ce {
                    ops.push(EditOp::RemoveElement { target: id_ref });
                    detail.push(format!(
                        "remove {what} window {cs:.3}s..{ce:.3}s",
                        what = clip.what()
                    ));
                } else if s <= cs {
                    // Head cut: the tail keeps showing the same frames.
                    let trim_start = shifted_trim(clip, cs, e)?;
                    ops.push(EditOp::Retiming {
                        target: id_ref,
                        start: Some(e),
                        duration: Some(ce - e),
                        trim_start,
                    });
                    detail.push(format!(
                        "shorten {what} to {e:.3}s..{ce:.3}s",
                        what = clip.what()
                    ));
                } else if e >= ce {
                    ops.push(EditOp::Retiming {
                        target: id_ref,
                        start: None,
                        duration: Some(s - cs),
                        trim_start: None,
                    });
                    detail.push(format!(
                        "shorten {what} to {cs:.3}s..{s:.3}s",
                        what = clip.what()
                    ));
                } else {
                    // Strictly inside: split at the range open, then cut
                    // the second half's head to the range close.
                    let trim_start = shifted_trim(clip, cs, e)?;
                    ops.push(EditOp::SplitElement {
                        target: id_ref.clone(),
                        at: s,
                    });
                    ops.push(EditOp::Retiming {
                        target: ClipRef {
                            id: None,
                            index: Some(i + 1),
                        },
                        start: Some(e),
                        duration: Some(ce - e),
                        trim_start,
                    });
                    detail.push(format!(
                        "cut {s:.3}s..{e:.3}s out of {what}",
                        what = clip.what()
                    ));
                }
            }
            detail.reverse();
            let summary = match target {
                Some(t) => format!("delete {t} range {rs:.3}s..{re:.3}s"),
                None if start.is_none() => "delete".to_string(),
                None => format!("delete range {rs:.3}s..{re:.3}s"),
            };
            Ok(Expanded {
                ops,
                affected,
                summary,
                detail,
            })
        }
        TimelineVerb::Set {
            target,
            property,
            value,
        } => {
            let idx = edits::resolve_clip(&comp.clips, target)?;
            let _ = idx;
            let (op, summary) = match property {
                SetProperty::Label => {
                    let label = if value.is_empty() {
                        None
                    } else {
                        Some(value.clone())
                    };
                    (
                        EditOp::SetLabel {
                            target: target.clone(),
                            label: label.clone(),
                        },
                        match label {
                            Some(l) => format!("label {target} as {l:?}"),
                            None => format!("clear label on {target}"),
                        },
                    )
                }
                SetProperty::Hidden => {
                    let hidden = match value.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => {
                            return Err(refused(
                                EditRefusalKind::BadValue,
                                E_SDK_BAD_VALUE,
                                format!(
                                    "timeline refused: `set hidden` value {value:?} is not `true` or `false`; write one of the two. Nothing was planned or applied, and no receipt was left."
                                ),
                            ));
                        }
                    };
                    (
                        EditOp::SetHidden {
                            target: target.clone(),
                            hidden,
                        },
                        if hidden {
                            format!("hide {target}")
                        } else {
                            format!("show {target}")
                        },
                    )
                }
                SetProperty::Start => {
                    let at: f64 = value.parse().unwrap_or(f64::NAN);
                    if !at.is_finite() || at < 0.0 {
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "timeline refused: `set start` value {value:?} is not finite seconds at or after 0. Nothing was planned or applied, and no receipt was left."
                            ),
                        ));
                    }
                    (
                        EditOp::Retiming {
                            target: target.clone(),
                            start: Some(at),
                            duration: None,
                            trim_start: None,
                        },
                        format!("set start of {target} to {at:.3}s"),
                    )
                }
                SetProperty::Duration => {
                    let dur: f64 = value.parse().unwrap_or(f64::NAN);
                    if !dur.is_finite() || dur <= 0.0 {
                        return Err(refused(
                            EditRefusalKind::BadValue,
                            E_SDK_BAD_VALUE,
                            format!(
                                "timeline refused: `set duration` value {value:?} is not a finite positive number of seconds. Nothing was planned or applied, and no receipt was left."
                            ),
                        ));
                    }
                    (
                        EditOp::Retiming {
                            target: target.clone(),
                            start: None,
                            duration: Some(dur),
                            trim_start: None,
                        },
                        format!("set duration of {target} to {dur:.3}s"),
                    )
                }
            };
            Ok(Expanded {
                ops: vec![op],
                affected: 1,
                summary: summary.clone(),
                detail: vec![summary],
            })
        }
        TimelineVerb::Duplicate { target, at, new_id } => {
            let idx = edits::resolve_clip(&comp.clips, target)?;
            let clip = &comp.clips[idx];
            if let Some(want) = new_id {
                edits::check_new_id(&comp.clips, &Some(want.clone()), "duplicated element")?;
            }
            let place = match at {
                Some(e) => resolve_time(e, comp, clip.start())?,
                None => clip.start() + clip.duration(),
            };
            if !place.is_finite() || place < 0.0 {
                return Err(refused(
                    EditRefusalKind::BadValue,
                    E_SDK_BAD_VALUE,
                    format!(
                        "timeline refused: duplicate of {target} resolves to {place}; write a film time at or after 0. Nothing was planned or applied, and no receipt was left."
                    ),
                ));
            }
            let summary = match new_id {
                Some(n) => format!("duplicate {target} at {place:.3}s as {n:?}"),
                None => format!("duplicate {target} at {place:.3}s"),
            };
            Ok(Expanded {
                ops: vec![EditOp::DuplicateElement {
                    target: target.clone(),
                    at: Some(place),
                    new_id: new_id.clone(),
                }],
                affected: 1,
                summary: summary.clone(),
                detail: vec![summary],
            })
        }
        TimelineVerb::Ids => {
            let rows = rows_of(comp);
            Ok(Expanded {
                ops: Vec::new(),
                affected: rows.len(),
                summary: "ids".to_string(),
                detail: rows
                    .iter()
                    .map(|r| {
                        format!(
                            "[{}] {} id={}",
                            r.index,
                            r.kind,
                            r.id.as_deref()
                                .map(|s| format!("{s:?}"))
                                .unwrap_or_else(|| "-".to_string())
                        )
                    })
                    .collect(),
            })
        }
    }
}

impl SdkSession {
    /// Every timeline row of the staged composition, in document
    /// order, each exposing `media_start` mapping file seconds to film
    /// seconds. Pure: reads the staged composition, changes nothing.
    pub fn timeline_rows(&self) -> Vec<TimelineRow> {
        rows_of(&self.composition)
    }

    /// Human-readable timeline rows of the staged composition, one
    /// header plus one `mediaStart` row per clip. Pure: changes
    /// nothing.
    pub fn timeline_rows_human(&self) -> String {
        format_rows_human(&self.composition)
    }

    /// Plan one verb (Sprint 15, T007): resolve every time expression,
    /// expand to concrete patch operations, and validate them against
    /// a scratch copy of the staged composition. Prints only a count
    /// ([`TimelinePlan`] display) and changes nothing — the session,
    /// the project tree, and history stay exactly as they were. An
    /// unresolvable expression or incapable verb refuses with a reason
    /// code and leaves no receipt.
    pub fn timeline_plan(&self, verb: &TimelineVerb) -> Result<TimelinePlan, EditRefusal> {
        let expanded = expand_verb(&self.composition, verb)?;
        let mut work = self.composition.clone();
        for op in &expanded.ops {
            edits::apply_forward(&mut work, op).map_err(|e| {
                refused(
                    e.kind,
                    e.code,
                    format!(
                        "timeline refused: plan of {} fails validation: {e} Nothing was planned or applied, and no receipt was left.",
                        expanded.summary
                    ),
                )
            })?;
        }
        Ok(TimelinePlan {
            verb_summary: expanded.summary,
            affected: expanded.affected,
            detail: expanded.detail,
        })
    }

    /// Apply one verb (Sprint 15, T007): plan first (resolving and
    /// validating everything before anything stages), then stage each
    /// patch through the same checks as [`SdkSession::apply`]. Returns
    /// a printable undo receipt carrying the staged sequence numbers.
    /// Stages in memory only — no project file is written, so the tree
    /// stays byte-identical until an explicit persist. A refused apply
    /// stages nothing, rolls back any partial staging, and leaves no
    /// receipt. `ids` lists identifiers and stages nothing.
    pub fn timeline_apply(&mut self, verb: TimelineVerb) -> Result<VerbReceipt, EditRefusal> {
        if let TimelineVerb::Ids = verb {
            let expanded = expand_verb(&self.composition, &TimelineVerb::Ids)?;
            return Ok(VerbReceipt {
                verb_summary: expanded.summary,
                applied: 0,
                seqs: Vec::new(),
                undo_available: self.last_verb_patches > 0,
                listing: expanded.detail,
            });
        }
        let expanded = expand_verb(&self.composition, &verb)?;
        // Validate the whole sequence against a scratch copy first: a
        // verb either stages completely or not at all.
        {
            let mut work = self.composition.clone();
            for op in &expanded.ops {
                edits::apply_forward(&mut work, op).map_err(|e| {
                    refused(
                        e.kind,
                        e.code,
                        format!(
                            "timeline refused: apply of {} fails validation: {e} Nothing was staged, and no receipt was left.",
                            expanded.summary
                        ),
                    )
                })?;
            }
        }
        // Replay onto the live session with rollback: the scratch run
        // passed, so this only guards against logic drift.
        let redo_saved = self.redo.clone();
        let dirty_saved = self.dirty;
        let next_seq_saved = self.next_seq;
        let mut seqs = Vec::new();
        for op in expanded.ops {
            match self.apply(op) {
                Ok(applied) => seqs.push(applied.seq),
                Err(e) => {
                    // Exact rollback: drop the staged tail in reverse.
                    for _ in 0..seqs.len() {
                        if let Some(patch) = self.edits.pop() {
                            let _ = edits::apply_revert(&mut self.composition, &patch.inverse);
                        }
                    }
                    self.redo = redo_saved;
                    self.dirty = dirty_saved;
                    self.next_seq = next_seq_saved;
                    return Err(refused(
                        e.kind,
                        e.code,
                        format!(
                            "timeline refused: apply of {} failed partway and rolled back: {e} Nothing stays staged, and no receipt was left.",
                            expanded.summary
                        ),
                    ));
                }
            }
        }
        self.last_verb_patches = seqs.len();
        Ok(VerbReceipt {
            verb_summary: expanded.summary,
            applied: seqs.len(),
            seqs,
            undo_available: true,
            listing: Vec::new(),
        })
    }

    /// Undo the last verb application (Sprint 15, T007): reverses
    /// exactly the patches that run staged, newest first, restoring
    /// the exact prior sources through the patch inverses. Refuses
    /// with [`E_SDK_NOTHING_TO_UNDO`] when no verb application is
    /// pending (nothing applied yet, or the last run is already
    /// undone), changing nothing.
    pub fn timeline_undo(&mut self) -> Result<VerbUndoReceipt, EditRefusal> {
        let pending = self.last_verb_patches;
        if pending == 0 {
            return Err(refused(
                EditRefusalKind::NothingToUndo,
                E_SDK_NOTHING_TO_UNDO,
                "timeline refused: undo names no verb application: no mutating verb ran since the last undo. Nothing was reversed.".to_string(),
            ));
        }
        if self.edits.len() < pending {
            return Err(refused(
                EditRefusalKind::NothingToUndo,
                E_SDK_NOTHING_TO_UNDO,
                format!(
                    "timeline refused: undo names {pending} verb patch(es) but the undo stack holds {}; its target is missing. Nothing was reversed.",
                    self.edits.len()
                ),
            ));
        }
        let mut summaries = Vec::new();
        for _ in 0..pending {
            match self.undo() {
                Ok(receipt) => summaries.push(receipt.summary),
                Err(e) => {
                    self.last_verb_patches -= summaries.len();
                    return Err(refused(
                        e.kind,
                        e.code,
                        format!(
                            "timeline refused: undo stopped partway: {e} {} patch(es) stay staged for a retry. Nothing further was reversed.",
                            pending - summaries.len()
                        ),
                    ));
                }
            }
        }
        self.last_verb_patches = 0;
        Ok(VerbUndoReceipt {
            undone: summaries.len(),
            summaries,
        })
    }
}
