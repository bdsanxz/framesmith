//! Clip gestures with ripple and links (Sprint 16, T006 / CAP-006,
//! J-02, J-06, STATE-03, INT-001, REC-001).
//!
//! The state every clip gesture reads: per-project link groups with a
//! ripple switch, gap highlights per track, and the out-of-sync badge.
//! Links and the ripple switch live in the project's hidden history
//! directory (`<project>/.framesmith/studio-links.json`), next to the
//! layout, theme, beats, and work-area files — never beside the sources
//! and never outside the project. Render scratch and caches stay
//! outside the project, as before; default renders (no SDK delta)
//! replay to themselves, so they stay byte-identical.
//!
//! Links name clips by declared `id`: a clip without one cannot join a
//! group, and a group whose members leave the timeline prunes itself on
//! the next mutation. Sync is a relative offset: the baseline records
//! each member's film start at link time, and a group is in sync while
//! every member still sits at its baseline offset from the group
//! anchor. Detached members (separated with `detach-audio`) may drift
//! freely and only raise the badge; every other member moves only with
//! its partners, so an edit touching a strict subset refuses as
//! sync-breaking before anything stages. Pure state edits (link,
//! unlink, detach, ripple switch) still record a named history turn
//! through the SDK machinery like every other Studio gesture; only the
//! composition patches they ride with (merge-back) move timing.
//!
//! Style boundary: link and gap payloads carry no style attributes, so
//! the CSS-like hyphenated KDL vocabulary and the fail-closed
//! `font-family` comma try-order pass through untouched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Link store file name inside the hidden project directory (next to
/// the layout, theme, beats, and work-area files).
pub const STUDIO_LINKS_FILE: &str = "studio-links.json";

/// Version of the persisted link store.
pub const STUDIO_LINKS_VERSION: u32 = 1;

/// Hidden link-store path for one project folder.
pub fn clip_links_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_LINKS_FILE)
}

/// Why a link edit refused.
fn refused_links(message: String) -> String {
    format!("studio refused: {message}")
}

/// One linked clip set (Sprint 16, T006): member `id` values with the
/// film start each member showed at link time (`baseline`) and the
/// members separated with `detach-audio` (`detached`), which may drift
/// and only raise the badge.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LinkGroup {
    /// Member clip `id` values, in link order.
    #[serde(default)]
    pub members: Vec<String>,
    /// Film start per member at link time (relative offsets define sync).
    #[serde(default)]
    pub baseline: BTreeMap<String, f64>,
    /// Members separated with `detach-audio`, allowed to drift.
    #[serde(default)]
    pub detached: Vec<String>,
}

/// The persisted link store (Sprint 16, T006): the ripple switch with
/// one group list per composition name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LinksFile {
    version: u32,
    #[serde(default = "ripple_default")]
    ripple: bool,
    #[serde(default)]
    links: BTreeMap<String, Vec<LinkGroup>>,
}

fn ripple_default() -> bool {
    true
}

/// The working link store: the ripple switch with one group list per
/// composition name.
#[derive(Debug, Clone, Default)]
pub struct ClipLinksStore {
    /// True while deletes and gap closes pull later clips earlier.
    pub ripple: bool,
    /// Link groups per composition name.
    pub groups: BTreeMap<String, Vec<LinkGroup>>,
}

/// What loading the link store reported (Sprint 16, T006): the store
/// to honor, whether it came from the project file, and the fallback
/// banner when it did not.
#[derive(Debug, Clone)]
pub struct ClipLinksLoad {
    /// The store to honor (defaults with ripple on when no file).
    pub store: ClipLinksStore,
    /// True when the store came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

impl ClipLinksLoad {
    fn fresh() -> Self {
        ClipLinksLoad {
            store: ClipLinksStore {
                ripple: true,
                groups: BTreeMap::new(),
            },
            restored: false,
            notice: None,
        }
    }

    fn fallback(folder: &Path, why: String) -> Self {
        let mut load = Self::fresh();
        load.notice = Some(format!(
            "studio: the link store {} {}; ripple stays on with no links.",
            clip_links_path_for(folder).display(),
            why
        ));
        load
    }
}

/// Load the per-project link store (Sprint 16, T006): a missing file
/// means ripple on with no links and no banner; a corrupt,
/// version-wrong, or invalid file falls back the same way and says so.
pub fn load_clip_links(folder: &Path) -> ClipLinksLoad {
    let path = clip_links_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ClipLinksLoad::fresh(),
        Err(e) => {
            return ClipLinksLoad::fallback(folder, format!("cannot be read ({e})"));
        }
        Ok(raw) => raw,
    };
    let file: LinksFile = match serde_json::from_str(&raw) {
        Err(e) => {
            return ClipLinksLoad::fallback(folder, format!("is not a link store ({e})"));
        }
        Ok(file) => file,
    };
    if file.version != STUDIO_LINKS_VERSION {
        return ClipLinksLoad::fallback(
            folder,
            format!(
                "carries version {}, this Studio reads version {}",
                file.version, STUDIO_LINKS_VERSION
            ),
        );
    }
    let mut groups = BTreeMap::new();
    for (comp, list) in file.links {
        let kept: Vec<LinkGroup> = list
            .into_iter()
            .map(|mut g| {
                g.members.sort();
                g.members.dedup();
                g.detached.retain(|d| g.members.contains(d));
                for m in &g.members {
                    g.baseline.entry(m.clone()).or_insert(0.0);
                }
                g.baseline.retain(|k, _| g.members.contains(k));
                g
            })
            .filter(|g| g.members.len() >= 2)
            .collect();
        if !kept.is_empty() {
            groups.insert(comp, kept);
        }
    }
    ClipLinksLoad {
        store: ClipLinksStore {
            ripple: file.ripple,
            groups,
        },
        restored: true,
        notice: None,
    }
}

/// Persist one link store (Sprint 16, T006): the whole store writes
/// back; a write failure refuses naming the file.
pub fn persist_clip_links(folder: &Path, store: &ClipLinksStore) -> Result<(), String> {
    let path = clip_links_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            refused_links(format!(
                "the hidden link directory {} cannot be prepared: {e}. Nothing was staged or recorded.",
                parent.display()
            ))
        })?;
    }
    let file = LinksFile {
        version: STUDIO_LINKS_VERSION,
        ripple: store.ripple,
        links: store.groups.clone(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| {
        refused_links(format!(
            "the link store cannot be encoded: {e}. Nothing was staged or recorded."
        ))
    })?;
    std::fs::write(&path, raw).map_err(|e| {
        refused_links(format!(
            "the link store {} cannot be written: {e}. Nothing was staged or recorded.",
            path.display()
        ))
    })
}

/// Declared `id` of one timeline row, when the clip carries one.
pub fn row_id_of(row: &sdk::TimelineRow) -> Option<&str> {
    row.id.as_deref()
}

/// Current film starts per member `id` of the open rows.
fn starts_by_id(rows: &[sdk::TimelineRow]) -> BTreeMap<String, f64> {
    let mut starts = BTreeMap::new();
    for row in rows {
        if let Some(id) = row_id_of(row) {
            starts.insert(id.to_string(), row.start);
        }
    }
    starts
}

/// True while every present member of the group still sits at its
/// baseline offset from the anchor (Sprint 16, T006): detached members
/// count too, so a drifted detach raises the badge until merge-back.
pub fn group_in_sync(group: &LinkGroup, rows: &[sdk::TimelineRow]) -> bool {
    let starts = starts_by_id(rows);
    let present: Vec<&String> = group
        .members
        .iter()
        .filter(|m| starts.contains_key(*m))
        .collect();
    if present.len() < 2 {
        return true;
    }
    let anchor = present[0];
    let anchor_now = starts[anchor];
    let anchor_was = group.baseline.get(anchor).copied().unwrap_or(anchor_now);
    for member in &present[1..] {
        let now = starts[*member];
        let was = group.baseline.get(*member).copied().unwrap_or(now);
        if (now - anchor_now - (was - anchor_was)).abs() > 1e-6 {
            return false;
        }
    }
    true
}

/// Prune members that left the timeline; drop groups below two
/// remaining members. Runs on every mutation so deleted clips never
/// haunt a group.
fn prune_groups(store: &mut ClipLinksStore, comp: &str, rows: &[sdk::TimelineRow]) {
    let starts = starts_by_id(rows);
    if let Some(list) = store.groups.get_mut(comp) {
        for group in list.iter_mut() {
            group.members.retain(|m| starts.contains_key(m));
            group.detached.retain(|d| group.members.contains(d));
            group.baseline.retain(|k, _| group.members.contains(k));
        }
        list.retain(|g| g.members.len() >= 2);
    }
    if store
        .groups
        .get(comp)
        .map(|l| l.is_empty())
        .unwrap_or(false)
    {
        store.groups.remove(comp);
    }
}

/// Refuse an edit touching a strict subset of a linked group (Sprint
/// 16, T006 / REC-001): every non-detached member moves only with its
/// non-detached partners, so touching one without the others names the
/// partners and moves nothing. Detached members drift freely and never
/// trigger this guard (the badge covers them instead).
pub fn check_linked_subset(
    store: &ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    touched_ids: &[String],
    what: &str,
) -> Result<(), String> {
    let Some(list) = store.groups.get(comp) else {
        return Ok(());
    };
    let starts = starts_by_id(rows);
    for group in list {
        let strict: Vec<&String> = group
            .members
            .iter()
            .filter(|m| starts.contains_key(*m) && !group.detached.contains(m))
            .collect();
        if strict.len() < 2 {
            continue;
        }
        let touched: Vec<&&String> = strict
            .iter()
            .filter(|m| touched_ids.iter().any(|t| t.as_str() == m.as_str()))
            .collect();
        if touched.is_empty() || touched.len() == strict.len() {
            continue;
        }
        let partners: Vec<String> = strict
            .iter()
            .filter(|m| !touched_ids.iter().any(|t| t.as_str() == m.as_str()))
            .map(|m| format!("{m:?}"))
            .collect();
        return Err(refused_links(format!(
            "the studio {what} would break sync with linked clip(s) {}; move them together with `multi-drag`, or unlink first. Nothing was staged or recorded.",
            partners.join(", ")
        )));
    }
    Ok(())
}

/// Link member `id` values into one group (Sprint 16, T006): at least
/// two distinct declared ids, every one on the timeline, none already
/// linked. The baseline records the current film starts, so the fresh
/// group reads in sync.
pub fn link_clips(
    store: &mut ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    members: &[String],
) -> Result<String, String> {
    let mut distinct = members.to_vec();
    distinct.sort();
    distinct.dedup();
    if distinct.len() < 2 {
        return Err(refused_links(
            "the studio `link` tool needs at least two different clips; pass distinct declared `id` values in `clips`. Nothing was staged or recorded.".to_string(),
        ));
    }
    let starts = starts_by_id(rows);
    for member in &distinct {
        if !starts.contains_key(member) {
            return Err(refused_links(format!(
                "the studio `link` tool names {member:?}, which is not a declared clip `id` on the timeline; link needs declared `id` values (clips without one cannot link). Nothing was staged or recorded."
            )));
        }
    }
    prune_groups(store, comp, rows);
    let list = store.groups.entry(comp.to_string()).or_default();
    for group in list.iter() {
        if let Some(hit) = group.members.iter().find(|m| distinct.contains(m)) {
            let partners: Vec<String> = group.members.iter().map(|m| format!("{m:?}")).collect();
            return Err(refused_links(format!(
                "the studio `link` tool names {hit:?}, already linked with {}; unlink it first. Nothing was staged or recorded.",
                partners.join(", ")
            )));
        }
    }
    let mut baseline = BTreeMap::new();
    for member in &distinct {
        baseline.insert(member.clone(), starts[member]);
    }
    let names: Vec<String> = distinct.iter().map(|m| format!("{m:?}")).collect();
    list.push(LinkGroup {
        members: distinct,
        baseline,
        detached: Vec::new(),
    });
    Ok(format!("link {}", names.join(", ")))
}

/// Remove members from their groups (Sprint 16, T006): every named id
/// must sit in a group, else the unlink names it and changes nothing.
/// Groups below two remaining members drop.
pub fn unlink_clips(
    store: &mut ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    members: &[String],
) -> Result<String, String> {
    if members.is_empty() {
        return Err(refused_links(
            "the studio `unlink` tool names no clip; pass a declared `id` in `clip` or `clips`. Nothing was staged or recorded.".to_string(),
        ));
    }
    prune_groups(store, comp, rows);
    let mut names = Vec::new();
    for member in members {
        let mut found = false;
        if let Some(list) = store.groups.get_mut(comp) {
            for group in list.iter_mut() {
                if group.members.contains(member) {
                    group.members.retain(|m| m != member);
                    group.detached.retain(|d| d != member);
                    group.baseline.remove(member);
                    found = true;
                    names.push(format!("{member:?}"));
                    break;
                }
            }
        }
        if !found {
            return Err(refused_links(format!(
                "the studio `unlink` tool names {member:?}, which sits in no link group; pass a linked clip `id`. Nothing was staged or recorded."
            )));
        }
    }
    prune_groups(store, comp, rows);
    Ok(format!("unlink {}", names.join(", ")))
}

/// Detach the audio of one member's group (Sprint 16, T006): the named
/// clip must sit in a group holding an audio-track member, else the
/// detach names the cause. Detached members drift freely and only raise
/// the badge.
pub fn detach_audio(
    store: &mut ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    member: &str,
) -> Result<String, String> {
    prune_groups(store, comp, rows);
    let Some(list) = store.groups.get_mut(comp) else {
        return Err(refused_links(format!(
            "the studio `detach-audio` tool names {member:?}, which sits in no link group; link picture and sound first. Nothing was staged or recorded."
        )));
    };
    let group = list
        .iter_mut()
        .find(|g| g.members.contains(&member.to_string()));
    let Some(group) = group else {
        return Err(refused_links(format!(
            "the studio `detach-audio` tool names {member:?}, which sits in no link group; link picture and sound first. Nothing was staged or recorded."
        )));
    };
    let audio: Vec<String> = group
        .members
        .iter()
        .filter(|m| {
            rows.iter()
                .any(|r| r.id.as_deref() == Some(m.as_str()) && r.track == "audio")
        })
        .cloned()
        .collect();
    if audio.is_empty() {
        let members: Vec<String> = group.members.iter().map(|m| format!("{m:?}")).collect();
        return Err(refused_links(format!(
            "the studio `detach-audio` tool names {member:?}, whose group holds no audio clip ({}); detach-audio separates picture from sound. Nothing was staged or recorded.",
            members.join(", ")
        )));
    }
    let fresh: Vec<String> = audio
        .iter()
        .filter(|m| !group.detached.contains(m))
        .cloned()
        .collect();
    if fresh.is_empty() {
        let names: Vec<String> = audio.iter().map(|m| format!("{m:?}")).collect();
        return Err(refused_links(format!(
            "the studio `detach-audio` tool names {}; already detached. Nothing was staged or recorded.",
            names.join(", ")
        )));
    }
    for m in &fresh {
        group.detached.push(m.clone());
    }
    let names: Vec<String> = fresh.iter().map(|m| format!("{m:?}")).collect();
    Ok(format!("detach-audio {}", names.join(", ")))
}

/// Plan a merge-back of two members (Sprint 16, T006): both must share
/// a group with a recorded baseline. Returns the film start the second
/// member must move to so its baseline offset from the first returns,
/// or `None` while the pair already sits in sync (only the detached
/// flags clear then).
pub fn merge_back_plan(
    store: &ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    first: &str,
    second: &str,
) -> Result<Option<f64>, String> {
    let starts = starts_by_id(rows);
    for id in [first, second] {
        if !starts.contains_key(id) {
            return Err(refused_links(format!(
                "the studio `merge-back` tool names {id:?}, which is not a declared clip `id` on the timeline; pass linked clip ids. Nothing was staged or recorded."
            )));
        }
    }
    let list = store.groups.get(comp).cloned().unwrap_or_default();
    let group = list
        .iter()
        .find(|g| {
            g.members.contains(&first.to_string()) && g.members.contains(&second.to_string())
        })
        .ok_or_else(|| {
            refused_links(format!(
                "the studio `merge-back` tool names {first:?} and {second:?}, which share no link baseline; link them first. Nothing was staged or recorded."
            ))
        })?;
    let anchor_now = starts[first];
    let anchor_was = group.baseline.get(first).copied().unwrap_or(anchor_now);
    let second_was = group
        .baseline
        .get(second)
        .copied()
        .unwrap_or(starts[second]);
    let want = anchor_now + (second_was - anchor_was);
    if (starts[second] - want).abs() <= 1e-6 {
        return Ok(None);
    }
    if !want.is_finite() || want < 0.0 {
        return Err(refused_links(format!(
            "the studio `merge-back` tool would park {second:?} at {want:.3}s, outside the timeline; move {first:?} onto the timeline first. Nothing was staged or recorded."
        )));
    }
    Ok(Some(want))
}

/// Clear the detached flags of two merged members (Sprint 16, T006):
/// runs after the restoring move lands, so the badge clears exactly
/// when the offsets return.
pub fn merge_back_clear(store: &mut ClipLinksStore, comp: &str, first: &str, second: &str) {
    if let Some(list) = store.groups.get_mut(comp) {
        for group in list.iter_mut() {
            if group.members.contains(&first.to_string())
                && group.members.contains(&second.to_string())
            {
                group.detached.retain(|d| d != first && d != second);
            }
        }
    }
}

/// One empty interval between consecutive clips on one track (Sprint
/// 16, T006): the gap highlight the panel offers to close.
#[derive(Debug, Clone)]
pub struct ClipGap {
    /// Track holding the gap (`video`, `audio`, `graphics`, ...).
    pub track: String,
    /// Film seconds where the gap opens (a clip edge).
    pub start: f64,
    /// Film seconds where the gap closes (the next clip edge).
    pub end: f64,
}

/// Gap highlights of the open rows (Sprint 16, T006): per track, in
/// film order, only intervals wider than a microsecond.
pub fn gaps_of(rows: &[sdk::TimelineRow]) -> Vec<ClipGap> {
    let mut by_track: BTreeMap<&str, Vec<&sdk::TimelineRow>> = BTreeMap::new();
    for row in rows {
        by_track.entry(row.track.as_str()).or_default().push(row);
    }
    let mut gaps = Vec::new();
    for (track, mut lanes) in by_track {
        lanes.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.index.cmp(&b.index))
        });
        let mut frontier = f64::NEG_INFINITY;
        for row in lanes {
            if row.start - frontier > 1e-6 && frontier.is_finite() {
                gaps.push(ClipGap {
                    track: track.to_string(),
                    start: frontier,
                    end: row.start,
                });
            }
            if row.end > frontier {
                frontier = row.end;
            }
        }
    }
    gaps.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.track.cmp(&b.track))
    });
    gaps
}

/// Render gap highlights as JSON for the gaps API.
pub fn gaps_json(gaps: &[ClipGap], ripple: bool) -> String {
    let mut items = String::new();
    for gap in gaps {
        if !items.is_empty() {
            items.push(',');
        }
        items.push_str(&format!(
            "{{\"track\":\"{}\",\"window\":[{:.3},{:.3}]}}",
            crate::studio::json_escape(&gap.track),
            gap.start,
            gap.end
        ));
    }
    format!("{{\"ripple\":{ripple},\"gaps\":[{items}]}}")
}

/// Render link groups as JSON for the links API (Sprint 16, T006):
/// members with detached flags, the in-sync verdict, and the
/// `out-of-sync` badge text exactly while the offsets drifted.
pub fn links_json(
    store: &ClipLinksStore,
    comp: &str,
    rows: &[sdk::TimelineRow],
    notice: Option<&str>,
) -> String {
    let starts = starts_by_id(rows);
    let mut items = String::new();
    if let Some(list) = store.groups.get(comp) {
        for group in list {
            if !items.is_empty() {
                items.push(',');
            }
            let present: Vec<&String> = group
                .members
                .iter()
                .filter(|m| starts.contains_key(*m))
                .collect();
            let in_sync = group_in_sync(group, rows);
            let mut members = String::new();
            for member in &present {
                if !members.is_empty() {
                    members.push(',');
                }
                let detached = group.detached.contains(*member);
                members.push_str(&format!(
                    "{{\"id\":\"{}\",\"detached\":{detached}}}",
                    crate::studio::json_escape(member)
                ));
            }
            let badge = if in_sync {
                "null".to_string()
            } else {
                "\"out-of-sync\"".to_string()
            };
            items.push_str(&format!(
                "{{\"members\":[{members}],\"in_sync\":{in_sync},\"badge\":{badge}}}"
            ));
        }
    }
    let notice_json = match notice {
        Some(n) => format!("\"{}\"", crate::studio::json_escape(n)),
        None => "null".to_string(),
    };
    format!(
        "{{\"ripple\":{},\"groups\":[{items}],\"notice\":{notice_json}}}",
        store.ripple
    )
}
