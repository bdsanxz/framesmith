//! Audio rack (Sprint 18, T007 / CAP-007, STATE-02, INT-001).
//!
//! The rack shapes sound visually: presets audition on the real mix
//! before anything writes, applying a preset lands as one named patch
//! turn, automation lanes (`volume`/`pan` key stops on sound-bearing
//! clips) rewrite points exactly across a window, audio groups move
//! together, and the master meter reads the true master level — the
//! same bytes a render mixes.
//!
//! Panel authority: the rack proposes edits and owns browsing state
//! (the preset catalog, the lane clipboard, group membership in the
//! hidden studio state file); the edit SDK disposes, so every applied
//! rack edit travels the Sprint 15 patch path and records one named
//! history turn, and undo walks it back exactly like any other edit.
//! Audition and the meter never write: they measure in-memory mixes
//! while the composition sources, the override delta, and history
//! stay byte-identical.
//!
//! Sound boundary: lane edits ride `volume` (0 silence to 2 twice
//! unity) and `pan` (−1 left to 1 right) on `audio` and `video`
//! clips only — the clips `check` accepts sound envelopes on. A lane
//! edit over silence (any other clip kind) refuses naming its kind,
//! and a window holding no stops reports the empty range instead of
//! inventing points.
//!
//! Style boundary: rack payloads carry no style attributes, so the
//! CSS-like hyphenated KDL vocabulary passes through untouched;
//! lane properties (`volume`, `pan`) and row properties (`volume`,
//! `fade-in`, `delay-mix`, ...) are hyphenated throughout, and the
//! rack never resolves, substitutes, or reorders `font-family`
//! try-order lists — it carries none.
//!
//! Storage boundary: the only project writes are validated turns
//! (the SDK override delta plus the history record, both under
//! `<project>/.framesmith/`) and the hidden rack state file
//! ([`STUDIO_AUDIO_FILE`]) beside the other studio stores. Audition
//! mixes, meter reads, and queue outputs never land beside the
//! sources — exactly like CLI renders.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Rack state file name inside the hidden project directory (next to
/// the link, layout, theme, beats, and work-area stores).
pub const STUDIO_AUDIO_FILE: &str = "studio-audio.json";

/// Version of the persisted rack state.
pub const STUDIO_AUDIO_VERSION: u32 = 1;

/// Hidden rack-state path for one project folder.
pub fn studio_audio_path_for(folder: &Path) -> PathBuf {
    folder.join(sdk::HISTORY_DIR_NAME).join(STUDIO_AUDIO_FILE)
}

/// Reason code: the preset name answers nothing in the catalog.
pub const E_RACK_UNKNOWN_PRESET: &str = "E_RACK_UNKNOWN_PRESET";
/// Reason code: a preset stage cannot run on its target.
pub const E_RACK_STAGE_FAILED: &str = "E_RACK_STAGE_FAILED";
/// Reason code: the lane window holds no stops to copy or rewrite.
pub const E_RACK_EMPTY_RANGE: &str = "E_RACK_EMPTY_RANGE";
/// Reason code: the lane target carries no sound.
pub const E_RACK_SILENT_TARGET: &str = "E_RACK_SILENT_TARGET";
/// Reason code: the audio group names nothing usable.
pub const E_RACK_BAD_GROUP: &str = "E_RACK_BAD_GROUP";
/// Reason code: the lane clipboard holds nothing to paste.
pub const E_RACK_EMPTY_CLIPBOARD: &str = "E_RACK_EMPTY_CLIPBOARD";
/// Reason code: the project cannot open for the rack.
pub const E_RACK_UNAVAILABLE: &str = "E_RACK_UNAVAILABLE";
/// Reason code: the mix cannot render (undecodable source or a
/// normalization that fails closed on silence).
pub const E_RACK_UNDECODABLE: &str = "E_RACK_UNDECODABLE";
/// Reason code: the lane or group target names no clip.
pub const E_RACK_UNKNOWN_TARGET: &str = "E_RACK_UNKNOWN_TARGET";
/// Reason code: a rack value breaks its rule.
pub const E_RACK_BAD_VALUE: &str = "E_RACK_BAD_VALUE";

/// Why a rack operation refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RackRefusalKind {
    /// The preset name answers nothing in the catalog.
    UnknownPreset,
    /// A preset stage cannot run on its target.
    StageFailed,
    /// The lane window holds no stops to copy or rewrite.
    EmptyRange,
    /// The lane target carries no sound.
    SilentTarget,
    /// The audio group names nothing usable.
    BadGroup,
    /// The lane clipboard holds nothing to paste.
    EmptyClipboard,
    /// The project cannot open for the rack.
    Unavailable,
    /// The mix cannot render.
    Undecodable,
    /// The lane or group target names no clip.
    UnknownTarget,
    /// A rack value breaks its rule.
    BadValue,
}

/// A refused rack operation: the kind, the reason code, and a message
/// naming the cause with the fix. Nothing starts on refusal: no patch
/// staged, no composition change, no history change, no state write.
#[derive(Debug, Clone)]
pub struct RackRefusal {
    pub kind: RackRefusalKind,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for RackRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

fn refused(kind: RackRefusalKind, code: &'static str, message: String) -> RackRefusal {
    RackRefusal {
        kind,
        code,
        message,
    }
}

/// Lift one SDK lane refusal into the rack reason space: empty
/// ranges and silent targets read uniformly on panel status, while
/// unknown targets and other bad values keep their SDK meaning under
/// rack codes. The message — naming the range, kind, or target —
/// passes through under the rack's voice.
fn lift_lane_refusal(e: sdk::EditRefusal) -> RackRefusal {
    let kind = if e.message.contains("the range is empty") {
        RackRefusalKind::EmptyRange
    } else if e.message.contains("range over silence") {
        RackRefusalKind::SilentTarget
    } else if e.kind == sdk::EditRefusalKind::UnknownTarget {
        RackRefusalKind::UnknownTarget
    } else {
        RackRefusalKind::BadValue
    };
    let code = match kind {
        RackRefusalKind::EmptyRange => E_RACK_EMPTY_RANGE,
        RackRefusalKind::SilentTarget => E_RACK_SILENT_TARGET,
        RackRefusalKind::UnknownTarget => E_RACK_UNKNOWN_TARGET,
        _ => E_RACK_BAD_VALUE,
    };
    RackRefusal {
        kind,
        code,
        message: format!(
            "rack refused: {}",
            e.message.trim_start_matches("sdk refused: ")
        ),
    }
}

/// One preset stage (Sprint 18, T007 / CAP-007): a named row write on
/// the target clip. `stage` names the chain step the refusal reports;
/// `property` is the CSS-like hyphenated row key (`volume`,
/// `fade-in`, `reverb`, ...); `value` its authored text.
#[derive(Debug, Clone)]
pub struct RackStage {
    /// Chain step name, reported when the stage cannot run.
    pub stage: String,
    /// Hyphenated row key the stage writes.
    pub property: String,
    /// Authored row value.
    pub value: String,
}

/// One named preset: a chain of row stages that audition on the real
/// mix and apply as one patch turn.
#[derive(Debug, Clone)]
pub struct RackPreset {
    /// Catalog name (`level-up`, `fade-ends`, ...).
    pub name: String,
    /// Chain steps in apply order.
    pub stages: Vec<RackStage>,
}

fn stage(stage: &str, property: &str, value: &str) -> RackStage {
    RackStage {
        stage: stage.to_string(),
        property: property.to_string(),
        value: value.to_string(),
    }
}

/// The audition-before-apply catalog (Sprint 18, T007): every stage
/// writes an audio row the SDK already validates, so an unrunnable
/// stage refuses naming itself with the SDK cause.
pub fn rack_presets() -> Vec<RackPreset> {
    vec![
        RackPreset {
            name: "level-up".to_string(),
            stages: vec![stage("level", "volume", "+3")],
        },
        RackPreset {
            name: "level-down".to_string(),
            stages: vec![stage("level", "volume", "-6")],
        },
        RackPreset {
            name: "fade-ends".to_string(),
            stages: vec![
                stage("fade-in", "fade-in", "0.5"),
                stage("fade-out", "fade-out", "0.5"),
            ],
        },
        RackPreset {
            name: "small-room".to_string(),
            stages: vec![
                stage("room", "reverb", "0.35"),
                stage("room-wet", "delay-mix", "0.15"),
            ],
        },
        RackPreset {
            name: "voice-focus".to_string(),
            stages: vec![
                stage("level", "volume", "+2"),
                stage("fade-in", "fade-in", "0.2"),
            ],
        },
        RackPreset {
            name: "silence-check".to_string(),
            stages: vec![stage("level", "volume", "-inf")],
        },
    ]
}

/// Look one preset up by name, or refuse listing the catalog.
pub fn preset_named(name: &str) -> Result<RackPreset, RackRefusal> {
    rack_presets()
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| {
            let mut names: Vec<String> =
                rack_presets().into_iter().map(|p| p.name).collect();
            names.sort();
            refused(
                RackRefusalKind::UnknownPreset,
                E_RACK_UNKNOWN_PRESET,
                format!(
                    "rack refused: preset {name:?} names nothing in the catalog; write one of {}. Nothing was applied, previewed, or recorded.",
                    names
                        .iter()
                        .map(|n| format!("{n:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })
}

/// Translate one preset to patch operations on its target: one
/// [`sdk::EditOp::SetRow`] per stage, in chain order.
pub fn preset_ops(preset: &RackPreset, target: &sdk::ClipRef) -> Vec<sdk::EditOp> {
    preset
        .stages
        .iter()
        .map(|s| sdk::EditOp::SetRow {
            target: sdk::ClipRef {
                id: target.id.clone(),
                index: target.index,
            },
            property: s.property.clone(),
            value: s.value.clone(),
        })
        .collect()
}

/// One master level reading (Sprint 18, T007 / CAP-007): the true
/// master peak of one mix, measured with the same sample math
/// `check` and the render summary use. A silent project reads exact
/// zeros with no fake movement; digital silence (`peak` 0) has no
/// finite dBFS and reports `dbfs` as `None`.
#[derive(Debug, Clone)]
pub struct MeterLevel {
    /// Sample frames the measured mix covers (stereo frames).
    pub sample_frames: usize,
    /// Largest absolute sample across both channels.
    pub peak: u32,
    /// Master level in dBFS (`None` for digital silence).
    pub dbfs: Option<f64>,
    /// Linear amplitude of the peak as a fraction of full scale.
    pub linear: f64,
    /// True while the master sits strictly above −1 dBFS.
    pub hot: bool,
    /// True only when the project mixes nothing (no audible source).
    pub silent: bool,
}

impl MeterLevel {
    /// The silent reading: exact zeros, no fake movement.
    pub fn silent() -> Self {
        Self {
            sample_frames: 0,
            peak: 0,
            dbfs: None,
            linear: 0.0,
            hot: false,
            silent: true,
        }
    }

    /// Human level for the meter face: `-3.2 dBFS`, or `-inf dBFS`
    /// for digital silence.
    pub fn level_text(&self) -> String {
        match self.dbfs {
            Some(db) => format!("{db:.1} dBFS"),
            None => "-inf dBFS".to_string(),
        }
    }
}

/// Measure one master level from mixed bytes: the peak the true
/// master carries, never a guess.
fn measure_mix(pcm: Option<&Vec<i16>>) -> MeterLevel {
    match pcm {
        None => MeterLevel::silent(),
        Some(bytes) => {
            let peak = render::peaks::peak_of(bytes);
            MeterLevel {
                sample_frames: bytes.len() / 2,
                peak,
                dbfs: render::peaks::dbfs_of(peak),
                linear: render::peaks::linear_of(peak),
                hot: render::peaks::is_hot(peak),
                silent: false,
            }
        }
    }
}

/// Open the rack read-only: validation first, then the effective
/// composition with the persisted delta replayed — and nothing
/// created, not even the history directory.
fn open_rack_read(
    folder: &Path,
    composition: Option<&str>,
) -> Result<sdk::SdkSession, RackRefusal> {
    sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            persist: false,
            history: false,
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused(
            RackRefusalKind::Unavailable,
            E_RACK_UNAVAILABLE,
            format!("rack refused: the project cannot open for the rack: {e}"),
        )
    })
}

/// Mix one composition with the render pipeline: the same
/// computation a full render mixes, so the rack hears true bytes.
/// `None` means the project mixes nothing; an undecodable source or
/// a normalization that fails closed on silence refuses naming the
/// cause.
fn mix_of(
    comp: &notation::model::Composition,
    all: &[notation::model::Composition],
    folder: &Path,
    ffmpeg: &Path,
) -> Result<Option<Vec<i16>>, RackRefusal> {
    render::render::build_mix_preview(comp, all, folder, ffmpeg).map_err(|e| {
        refused(
            RackRefusalKind::Undecodable,
            E_RACK_UNDECODABLE,
            format!(
                "rack refused: the mix cannot render: {}. Nothing was written.",
                e.message()
            ),
        )
    })
}

/// Read the master meter (Sprint 18, T007 / CAP-007): the true master
/// level of the open composition — the peak of the same bytes a full
/// render mixes, measured with the same sample math `check` reports.
/// A project with no audio reads exact silence with no fake movement;
/// an undecodable mix refuses naming the file. Reads only: the
/// composition sources, the override delta, and history stay
/// byte-identical.
pub fn read_master_meter(
    folder: &Path,
    composition: Option<&str>,
    ffmpeg: &Path,
) -> Result<MeterLevel, RackRefusal> {
    let session = open_rack_read(folder, composition)?;
    let pcm = mix_of(
        session.effective_composition(),
        session.all_compositions(),
        folder,
        ffmpeg,
    )?;
    Ok(measure_mix(pcm.as_ref()))
}

/// One preset audition (Sprint 18, T007 / CAP-007): the before and
/// after master levels of the preset on the real mix, with whether a
/// single sample changed. Audition previews only — the project keeps
/// its truth: no composition write, no delta write, no history turn.
#[derive(Debug, Clone)]
pub struct AuditionReport {
    /// Preset auditioned.
    pub preset: String,
    /// Target the stages address.
    pub target: String,
    /// One-line summary per applied stage, in chain order.
    pub stages: Vec<String>,
    /// Master level before the preset.
    pub before: MeterLevel,
    /// Master level with the preset on the real mix.
    pub after: MeterLevel,
    /// True when any mixed sample changed.
    pub changed: bool,
}

/// Audition one preset on the real mix without writing (Sprint 18,
/// T007): the stages apply to an in-memory copy of the effective
/// composition through the same forward path applying edits use,
/// then both mixes render and measure. A stage that cannot run
/// refuses naming the preset and the stage with the SDK cause while
/// the project stays exactly as it was.
pub fn audition_preset(
    folder: &Path,
    composition: Option<&str>,
    preset_name: &str,
    target: &sdk::ClipRef,
    ffmpeg: &Path,
) -> Result<AuditionReport, RackRefusal> {
    let preset = preset_named(preset_name)?;
    // Writable session, history off, never persisted: stages apply in
    // memory only, so the delta file and history stay untouched and
    // the open itself creates nothing.
    let mut session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            history: false,
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused(
            RackRefusalKind::Unavailable,
            E_RACK_UNAVAILABLE,
            format!("rack refused: the project cannot open for the rack: {e}"),
        )
    })?;
    let before_pcm = mix_of(
        session.effective_composition(),
        session.all_compositions(),
        folder,
        ffmpeg,
    )?;
    let ops = preset_ops(&preset, target);
    let mut stages = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        match session.apply(op.clone()) {
            Ok(applied) => stages.push(applied.op.summary()),
            Err(e) => {
                let stage = preset
                    .stages
                    .get(i)
                    .map(|s| s.stage.as_str())
                    .unwrap_or("?");
                return Err(refused(
                    RackRefusalKind::StageFailed,
                    E_RACK_STAGE_FAILED,
                    format!(
                        "rack refused: preset `{}` stage `{stage}` cannot run: {e} Nothing was persisted or recorded.",
                        preset.name
                    ),
                ));
            }
        }
    }
    let modified = session.effective_composition().clone();
    let mut all: Vec<notation::model::Composition> = session.all_compositions().to_vec();
    if let Some(slot) = all.iter_mut().find(|c| c.name == modified.name) {
        *slot = modified.clone();
    } else {
        all.push(modified.clone());
    }
    let after_pcm = mix_of(&modified, &all, folder, ffmpeg)?;
    let changed = before_pcm != after_pcm;
    Ok(AuditionReport {
        preset: preset.name,
        target: target.to_string(),
        stages,
        before: measure_mix(before_pcm.as_ref()),
        after: measure_mix(after_pcm.as_ref()),
        changed,
    })
}

/// One applied preset: the patch-turn receipt.
#[derive(Debug, Clone)]
pub struct PresetReceipt {
    /// Preset applied.
    pub preset: String,
    /// Patches the turn holds (one per stage).
    pub applied: usize,
    /// Recorded history-turn sequence.
    pub turn: u64,
    /// Actor the turn is recorded under.
    pub actor: String,
    /// One-line summary per applied stage, in chain order.
    pub summaries: Vec<String>,
}

/// Apply one preset as a patch turn (Sprint 18, T007 / CAP-007): the
/// stages apply through the Sprint 15 patch machinery and one named
/// history turn records them, so walking undo back reverses the whole
/// preset exactly like any other turn.
/// A stage that cannot run refuses naming the preset and the stage
/// with the SDK cause; the dropped session stages nothing, so the
/// composition, the delta, and history stay exactly as they were.
pub fn apply_preset(
    folder: &Path,
    composition: Option<&str>,
    preset_name: &str,
    target: &sdk::ClipRef,
    actor: &str,
) -> Result<PresetReceipt, RackRefusal> {
    let preset = preset_named(preset_name)?;
    let what = format!("preset `{}`", preset.name);
    let ops = preset_ops(&preset, target);
    let names: Vec<String> = preset.stages.iter().map(|s| s.stage.clone()).collect();
    let receipt = apply_ops_as_turn(folder, composition, &ops, &what, &names, actor)?;
    Ok(PresetReceipt {
        preset: preset.name,
        applied: receipt.applied,
        turn: receipt.turn,
        actor: receipt.actor,
        summaries: receipt.summaries,
    })
}

/// One applied rack turn: what landed and where it recorded.
#[derive(Debug, Clone)]
pub struct RackTurnReceipt {
    /// What applied (`preset `x``, `lane ramp ...`, ...).
    pub what: String,
    /// Patches the turn holds.
    pub applied: usize,
    /// Recorded history-turn sequence.
    pub turn: u64,
    /// Actor the turn is recorded under.
    pub actor: String,
    /// One-line summary per applied patch, in order.
    pub summaries: Vec<String>,
}

/// Apply caller-built operations as one named patch turn: shared
/// machinery for presets (one op per stage) and group fan-out (one
/// lane op per member). A failing operation refuses naming its step
/// with the SDK cause; the dropped session stages nothing, so the
/// composition, the delta, and history stay exactly as they were.
fn apply_ops_as_turn(
    folder: &Path,
    composition: Option<&str>,
    ops: &[sdk::EditOp],
    what: &str,
    steps: &[String],
    actor: &str,
) -> Result<RackTurnReceipt, RackRefusal> {
    let mut session = sdk::SdkSession::open(
        folder,
        sdk::OpenOptions {
            composition: composition.map(str::to_string),
            ..sdk::OpenOptions::default()
        },
    )
    .map_err(|e| {
        refused(
            RackRefusalKind::Unavailable,
            E_RACK_UNAVAILABLE,
            format!("rack refused: the project cannot open for the rack: {e}"),
        )
    })?;
    let mut summaries = Vec::with_capacity(ops.len());
    for (i, op) in ops.iter().enumerate() {
        match session.apply(op.clone()) {
            Ok(applied) => summaries.push(applied.op.summary()),
            Err(e) => {
                let step = steps.get(i).map(|s| s.as_str()).unwrap_or("?");
                return Err(refused(
                    RackRefusalKind::StageFailed,
                    E_RACK_STAGE_FAILED,
                    format!(
                        "rack refused: {what} step `{step}` cannot run: {e} Nothing was persisted or recorded."
                    ),
                ));
            }
        }
    }
    let applied = summaries.len();
    match session.persist_turn(actor) {
        Ok(receipt) => Ok(RackTurnReceipt {
            what: what.to_string(),
            applied,
            turn: receipt.turn.unwrap_or(0),
            actor: receipt.actor.unwrap_or_else(|| actor.to_string()),
            summaries,
        }),
        Err(e) => Err(refused(
            RackRefusalKind::StageFailed,
            E_RACK_STAGE_FAILED,
            format!("rack refused: {what} cannot record its turn: {e}"),
        )),
    }
}

/// The persisted audio-group store (Sprint 18, T007): one named
/// member-id list per composition, in the hidden rack state file.
/// Groups move together: a lane edit addressing one member fans out
/// to every member as one patch turn.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct AudioGroupsFile {
    version: u32,
    #[serde(default)]
    groups: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// The working audio-group registry: member `id` values per group
/// name per composition name.
#[derive(Debug, Clone, Default)]
pub struct AudioGroups {
    /// Member clip `id` values per group per composition.
    pub groups: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

/// What loading the group registry reported: the registry to honor,
/// whether it came from the project file, and the fallback banner
/// when it did not.
#[derive(Debug, Clone)]
pub struct AudioGroupsLoad {
    /// The registry to honor (empty when no file).
    pub groups: AudioGroups,
    /// True when the registry came from the project file.
    pub restored: bool,
    /// Fallback banner, set exactly when the saved file cannot restore.
    pub notice: Option<String>,
}

/// Load the audio-group registry: missing files load empty (no
/// groups yet is not an error); an unreadable or versioned file
/// falls back to empty with a banner, never a guess.
pub fn load_audio_groups(folder: &Path) -> AudioGroupsLoad {
    let path = studio_audio_path_for(folder);
    let fresh = || AudioGroupsLoad {
        groups: AudioGroups::default(),
        restored: false,
        notice: None,
    };
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return fresh(),
        Err(e) => {
            let mut load = fresh();
            load.notice = Some(format!(
                "studio: the audio-group store {} cannot be read ({e}); starting with no groups.",
                path.display()
            ));
            return load;
        }
    };
    match serde_json::from_str::<AudioGroupsFile>(&raw) {
        Ok(file) if file.version == STUDIO_AUDIO_VERSION => AudioGroupsLoad {
            groups: AudioGroups {
                groups: file.groups,
            },
            restored: true,
            notice: None,
        },
        _ => {
            let mut load = fresh();
            load.notice = Some(format!(
                "studio: the audio-group store {} is not version {}; starting with no groups.",
                path.display(),
                STUDIO_AUDIO_VERSION
            ));
            load
        }
    }
}

/// Write the audio-group registry to the hidden rack state file: the
/// only group write besides validated turns, never beside the
/// sources and never outside the project.
fn save_audio_groups(folder: &Path, groups: &AudioGroups) -> Result<(), String> {
    let dir = studio_audio_path_for(folder)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| folder.to_path_buf());
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "studio: the audio-group store directory {} cannot be prepared ({e}). Nothing was written.",
            dir.display()
        )
    })?;
    let file = AudioGroupsFile {
        version: STUDIO_AUDIO_VERSION,
        groups: groups.groups.clone(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|e| {
        format!("studio: the audio groups cannot serialize ({e}). Nothing was written.")
    })?;
    std::fs::write(studio_audio_path_for(folder), raw).map_err(|e| {
        format!(
            "studio: the audio-group store {} cannot be written ({e}). Nothing was written.",
            studio_audio_path_for(folder).display()
        )
    })
}

/// Check one group definition against the open composition: the name
/// is non-empty, at least one member rides along, and every member
/// is a declared `id` on a sound-bearing clip. Anything else refuses
/// naming the offender, never guessed past.
fn check_group_members(
    comp: &notation::model::Composition,
    name: &str,
    members: &[String],
) -> Result<(), RackRefusal> {
    if name.trim().is_empty() {
        return Err(refused(
            RackRefusalKind::BadGroup,
            E_RACK_BAD_GROUP,
            "rack refused: the audio group names nothing; pass a non-empty group name. Nothing was applied, previewed, or recorded.".to_string(),
        ));
    }
    if members.is_empty() {
        return Err(refused(
            RackRefusalKind::BadGroup,
            E_RACK_BAD_GROUP,
            format!(
                "rack refused: audio group {name:?} rides with no members; pass at least one clip `id`. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    for id in members {
        match comp.clips.iter().find(|c| c.id() == Some(id.as_str())) {
            None => {
                return Err(refused(
                    RackRefusalKind::BadGroup,
                    E_RACK_BAD_GROUP,
                    format!(
                        "rack refused: audio group {name:?} member {id:?} names no clip in the open composition; pass declared `id` values. Nothing was applied, previewed, or recorded."
                    ),
                ));
            }
            Some(clip) => {
                if !matches!(
                    clip,
                    notation::model::Clip::Audio(_) | notation::model::Clip::Video(_)
                ) {
                    return Err(refused(
                        RackRefusalKind::BadGroup,
                        E_RACK_BAD_GROUP,
                        format!(
                            "rack refused: audio group {name:?} member {id:?} is a {}; groups move sound-bearing clips (`audio` and `video`) only. Nothing was applied, previewed, or recorded.",
                            clip.what()
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Define one audio group: validate the members against the open
/// composition, then persist the registry to the hidden rack state.
/// Pure state (no composition patch), like the other studio stores.
pub fn define_audio_group(
    folder: &Path,
    composition: Option<&str>,
    name: &str,
    members: &[String],
) -> Result<Vec<String>, RackRefusal> {
    let session = open_rack_read(folder, composition)?;
    let comp = session.effective_composition();
    check_group_members(comp, name, members)?;
    let mut load = load_audio_groups(folder);
    load.groups
        .groups
        .entry(comp.name.clone())
        .or_default()
        .insert(name.to_string(), members.to_vec());
    save_audio_groups(folder, &load.groups)
        .map_err(|message| refused(RackRefusalKind::BadGroup, E_RACK_BAD_GROUP, message))?;
    Ok(members.to_vec())
}

/// Disband one audio group: forget the name, keep every clip. Pure
/// state; refuses naming the missing group.
pub fn disband_audio_group(
    folder: &Path,
    composition: Option<&str>,
    name: &str,
) -> Result<(), RackRefusal> {
    let session = open_rack_read(folder, composition)?;
    let comp_name = session.effective_composition().name.clone();
    let mut load = load_audio_groups(folder);
    let removed = load
        .groups
        .groups
        .get_mut(&comp_name)
        .and_then(|by_name| by_name.remove(name))
        .is_some();
    if !removed {
        return Err(refused(
            RackRefusalKind::BadGroup,
            E_RACK_BAD_GROUP,
            format!(
                "rack refused: audio group {name:?} names nothing on composition {comp_name:?}; nothing disbands. Nothing was applied, previewed, or recorded."
            ),
        ));
    }
    save_audio_groups(folder, &load.groups)
        .map_err(|message| refused(RackRefusalKind::BadGroup, E_RACK_BAD_GROUP, message))
}

/// Resolve one lane target through its group (Sprint 18, T007): when
/// the target `id` rides in an audio group on this composition, every
/// member moves together; otherwise the target moves alone. A member
/// that left the timeline refuses naming it instead of moving a
/// partial group.
fn resolve_lane_targets(
    comp: &notation::model::Composition,
    groups: &AudioGroups,
    target: &sdk::ClipRef,
) -> Result<Vec<sdk::ClipRef>, RackRefusal> {
    let member_of = target.id.as_deref().and_then(|id| {
        groups
            .groups
            .get(&comp.name)
            .and_then(|by_name| {
                by_name
                    .iter()
                    .find(|(_, members)| members.iter().any(|m| m == id))
            })
            .map(|(group, members)| (group.clone(), members.clone()))
    });
    match member_of {
        None => Ok(vec![sdk::ClipRef {
            id: target.id.clone(),
            index: target.index,
        }]),
        Some((group, members)) => {
            for id in &members {
                if !comp.clips.iter().any(|c| c.id() == Some(id.as_str())) {
                    return Err(refused(
                        RackRefusalKind::BadGroup,
                        E_RACK_BAD_GROUP,
                        format!(
                            "rack refused: audio group {group:?} member {id:?} left the timeline; the group no longer moves together until it is redefined. Nothing was applied, previewed, or recorded."
                        ),
                    ));
                }
            }
            Ok(members
                .into_iter()
                .map(|id| sdk::ClipRef {
                    id: Some(id),
                    index: None,
                })
                .collect())
        }
    }
}

/// Rewrite one automation lane across a window, through groups
/// (Sprint 18, T007 / CAP-007): one [`sdk::EditOp::RewriteLane`] per
/// member as a single patch turn, so grouped lanes move together and
/// one undo reverses them all. Stops outside the window keep their
/// exact instant, value, and easing on every member; an empty window
/// reports the range instead of inventing points.
#[allow(clippy::too_many_arguments)]
pub fn rewrite_lane(
    folder: &Path,
    composition: Option<&str>,
    target: &sdk::ClipRef,
    property: &str,
    start: f64,
    end: f64,
    edit: &sdk::LaneEdit,
    actor: &str,
) -> Result<RackTurnReceipt, RackRefusal> {
    let probe = open_rack_read(folder, composition)?;
    let members = resolve_lane_targets(
        probe.effective_composition(),
        &load_audio_groups(folder).groups,
        target,
    )?;
    let what = match edit {
        sdk::LaneEdit::Ramp { .. } => "lane ramp",
        sdk::LaneEdit::Swell { .. } => "lane swell",
        sdk::LaneEdit::Dip { .. } => "lane dip",
        sdk::LaneEdit::Simplify { .. } => "lane simplify",
    };
    let ops: Vec<sdk::EditOp> = members
        .iter()
        .map(|member| sdk::EditOp::RewriteLane {
            target: member.clone(),
            property: property.to_string(),
            start,
            end,
            edit: edit.clone(),
        })
        .collect();
    let steps: Vec<String> = members.iter().map(|m| m.to_string()).collect();
    apply_ops_as_turn(
        folder,
        composition,
        &ops,
        &format!("{what} `{property}` across {start}..{end}"),
        &steps,
        actor,
    )
    .map_err(|e| {
        // The SDK stage wrapper names the step; the lane cause inside
        // lifts into the rack reason space.
        lift_lane_stage_refusal(e)
    })
}

/// Lift one rack stage refusal carrying an SDK lane cause: the stage
/// wrapper names the failed step, so the lift reads the cause out of
/// the message tail.
fn lift_lane_stage_refusal(e: RackRefusal) -> RackRefusal {
    let kind = if e.message.contains("the range is empty") {
        RackRefusalKind::EmptyRange
    } else if e.message.contains("range over silence") {
        RackRefusalKind::SilentTarget
    } else if e.message.contains("E_SDK_UNKNOWN_TARGET") {
        RackRefusalKind::UnknownTarget
    } else {
        RackRefusalKind::BadValue
    };
    let code = match kind {
        RackRefusalKind::EmptyRange => E_RACK_EMPTY_RANGE,
        RackRefusalKind::SilentTarget => E_RACK_SILENT_TARGET,
        RackRefusalKind::UnknownTarget => E_RACK_UNKNOWN_TARGET,
        _ => E_RACK_BAD_VALUE,
    };
    RackRefusal {
        kind,
        code,
        message: e.message,
    }
}

/// One lane clipboard (Sprint 18, T007): copied stops with the lane
/// they came from, ready to paste back exactly. Panel browsing state,
/// like selections and queues — never project truth.
#[derive(Debug, Clone, Default)]
pub struct LaneClipboard {
    /// Lane property the stops ride (`volume` or `pan`).
    pub property: String,
    /// Copied stops in rising time order, instants and values exact.
    pub stops: Vec<sdk::KeyStopInput>,
    /// Target the stops were copied from.
    pub source: String,
}

impl LaneClipboard {
    /// True while the clipboard holds stops ready to paste.
    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }
}

/// Copy one lane range to the clipboard: the stops inside
/// `[start, end]` with their exact instants, values, and easings.
/// Stages nothing; an empty range refuses naming it instead of
/// copying silence.
pub fn copy_lane(
    folder: &Path,
    composition: Option<&str>,
    target: &sdk::ClipRef,
    property: &str,
    start: f64,
    end: f64,
) -> Result<LaneClipboard, RackRefusal> {
    let session = open_rack_read(folder, composition)?;
    match sdk::edits::copy_lane_range(
        session.effective_composition(),
        target,
        property,
        start,
        end,
    ) {
        Ok(stops) => Ok(LaneClipboard {
            property: property.trim().replace('_', "-"),
            stops,
            source: target.to_string(),
        }),
        Err(e) => Err(lift_lane_refusal(e)),
    }
}

/// Paste the clipboard onto one lane as a patch turn: the copied
/// stops land as one fresh key while the span they cover trims from
/// the existing keys exactly like a rewrite, so the round trip
/// rewrites points exactly. An empty clipboard refuses instead of
/// pasting silence.
pub fn paste_lane(
    folder: &Path,
    composition: Option<&str>,
    clipboard: &LaneClipboard,
    target: &sdk::ClipRef,
    actor: &str,
) -> Result<RackTurnReceipt, RackRefusal> {
    if clipboard.is_empty() {
        return Err(refused(
            RackRefusalKind::EmptyClipboard,
            E_RACK_EMPTY_CLIPBOARD,
            "rack refused: the lane clipboard holds no stops; copy a lane range before pasting. Nothing was applied, previewed, or recorded.".to_string(),
        ));
    }
    let probe = open_rack_read(folder, composition)?;
    let members = resolve_lane_targets(
        probe.effective_composition(),
        &load_audio_groups(folder).groups,
        target,
    )?;
    let ops: Vec<sdk::EditOp> = members
        .iter()
        .map(|member| sdk::EditOp::PasteLane {
            target: member.clone(),
            property: clipboard.property.clone(),
            stops: clipboard.stops.clone(),
        })
        .collect();
    let steps: Vec<String> = members.iter().map(|m| m.to_string()).collect();
    apply_ops_as_turn(
        folder,
        composition,
        &ops,
        &format!("lane paste of {} stop(s)", clipboard.stops.len()),
        &steps,
        actor,
    )
}
