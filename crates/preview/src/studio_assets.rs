//! Assets panel for the Studio shell (Sprint 18, T002 / CAP-002,
//! J-02, STATE-01, STATE-04, REC-003).
//!
//! The maker imports or drops media, searches, filters by type or
//! used state, switches project and all-projects views, and picks
//! context actions (insert, reveal, rename, remove). Image, video,
//! audio, font, and LUT cube assets list with truthful used state:
//! every project-relative path the validated compositions reference
//! (clip `path`, `font` declarations, grade `lut` sources, box
//! image fills, VFX matte and displacement maps, and image variable
//! defaults) marks its file used and names its referrers.
//!
//! The panel proposes, the SDK disposes: only `insert` applies
//! through the Sprint 15 patch and history machinery
//! ([`crate::studio::apply_studio_edit`]), so inserts share one undo
//! system with every other panel edit and land as named history
//! turns. Import, rename, and remove are project file operations
//! with receipts, like headless file verbs: the shared history only
//! tracks composition sources and hidden deltas, never media bytes,
//! so these actions report what they did instead of recording a
//! turn. A refused action names its cause while the project and the
//! history stay exactly as they were.
//!
//! Import boundary: a drop lands through the import endpoint with
//! the OS file path; the copy stores images, video, and audio under
//! `media/`, fonts under `fonts/`, and LUT cubes at the project
//! root (the spellings `check` fix messages name). A stored name
//! that already holds identical bytes keeps the existing file and
//! says so; the same name with different bytes gains a numeric
//! suffix, so imports never overwrite. Queue outputs still land
//! outside the project sources exactly like CLI renders; default
//! renders stay byte-identical.
//!
//! View boundary: the project view lists the served project; the
//! all-projects view lists every sibling project holding a
//! `composition.kdl`, each with its own used state. Other projects
//! are read-only: any action naming one refuses instead of writing.
//!
//! Style boundary: request keys, response keys, and asset type names
//! stay CSS-like hyphenated end to end (`used-by`, `stored-name`,
//! `all-projects`, `font-family`); `font-family` try-order lists
//! pass through untouched with fail-closed resolution at render
//! time, never resolved or substituted here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::studio::json_escape;

/// Image sources the panel lists: still pictures `check` decodes,
/// including SVG (which inserts as an `image` clip like the notation
/// accepts).
pub const ASSET_IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "heic", "heif", "svg"];
/// Video containers the panel lists.
pub const ASSET_VIDEO_EXTS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi", "ogv"];
/// Audio sources the panel lists (mirrors the Sprint 3 mix set).
pub const ASSET_AUDIO_EXTS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "mka", "ac3",
    "mp2", "amr",
];
/// Font files the panel lists (TrueType and OpenType, like `check`).
pub const ASSET_FONT_EXTS: &[&str] = &["ttf", "otf"];
/// LUT cubes the panel lists.
pub const ASSET_LUT_EXTS: &[&str] = &["cube"];

/// One asset type the panel manages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetKind {
    Image,
    Video,
    Audio,
    Font,
    Lut,
}

impl AssetKind {
    /// Served spelling of the kind (`image`, `video`, ...).
    pub fn name(self) -> &'static str {
        match self {
            AssetKind::Image => "image",
            AssetKind::Video => "video",
            AssetKind::Audio => "audio",
            AssetKind::Font => "font",
            AssetKind::Lut => "lut",
        }
    }

    /// Parse one served kind spelling; `None` names nothing.
    pub fn parse(raw: &str) -> Option<AssetKind> {
        match raw.trim().to_lowercase().as_str() {
            "image" => Some(AssetKind::Image),
            "video" => Some(AssetKind::Video),
            "audio" => Some(AssetKind::Audio),
            "font" => Some(AssetKind::Font),
            "lut" => Some(AssetKind::Lut),
            _ => None,
        }
    }

    /// Accepted file extensions of the kind, lowercase, no dots.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            AssetKind::Image => ASSET_IMAGE_EXTS,
            AssetKind::Video => ASSET_VIDEO_EXTS,
            AssetKind::Audio => ASSET_AUDIO_EXTS,
            AssetKind::Font => ASSET_FONT_EXTS,
            AssetKind::Lut => ASSET_LUT_EXTS,
        }
    }

    /// Project subdirectory imports store into: `media` for picture
    /// and sound, `fonts` for fonts, and the project root for LUT
    /// cubes (the spellings `check` fix messages name).
    pub fn dir(self) -> &'static str {
        match self {
            AssetKind::Image | AssetKind::Video | AssetKind::Audio => "media",
            AssetKind::Font => "fonts",
            AssetKind::Lut => "",
        }
    }
}

/// Every kind the panel manages, in listing order.
pub const ASSET_KINDS: &[AssetKind] = &[
    AssetKind::Image,
    AssetKind::Video,
    AssetKind::Audio,
    AssetKind::Font,
    AssetKind::Lut,
];

/// Classify one file name by extension (case-insensitive); `None`
/// names no managed asset type.
pub fn asset_kind_of(file_name: &str) -> Option<AssetKind> {
    let lower = file_name.to_lowercase();
    for kind in ASSET_KINDS {
        if kind
            .extensions()
            .iter()
            .any(|ext| lower.ends_with(&format!(".{ext}")))
        {
            return Some(*kind);
        }
    }
    None
}

/// Human text of every accepted type, for refusals: names the types
/// and their extensions so the maker knows what imports.
pub fn accepted_types_text() -> String {
    ASSET_KINDS
        .iter()
        .map(|k| {
            format!(
                "{} ({})",
                k.name(),
                k.extensions()
                    .iter()
                    .map(|e| format!(".{e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn refused_assets(message: String) -> String {
    format!("studio refused: {message}")
}

/// One listed asset: its project-relative name, kind, size, and
/// truthful used state with its referrers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    /// Project-relative path with `/` separators (`media/card.png`).
    pub name: String,
    /// Managed kind of the file.
    pub kind: AssetKind,
    /// File size in bytes.
    pub bytes: u64,
    /// True while a validated composition references the file.
    pub used: bool,
    /// Who references the file (`composition "t" image clip id "hero"`),
    /// sorted; empty while unused.
    pub used_by: Vec<String>,
}

/// Render one entry as JSON for the assets API.
pub fn asset_entry_json(entry: &AssetEntry) -> String {
    format!(
        "{{\"name\":\"{}\",\"type\":\"{}\",\"bytes\":{},\"used\":{},\"used-by\":[{}]}}",
        json_escape(&entry.name),
        entry.kind.name(),
        entry.bytes,
        if entry.used { "true" } else { "false" },
        entry
            .used_by
            .iter()
            .map(|r| format!("\"{}\"", json_escape(r)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// The VFX chain of one clip, whatever its kind.
fn clip_vfx(clip: &notation::model::Clip) -> &notation::model::VfxChain {
    match clip {
        notation::model::Clip::Text(c) => &c.vfx,
        notation::model::Clip::Video(c) => &c.vfx,
        notation::model::Clip::Box(c) => &c.vfx,
        notation::model::Clip::Flex(c) => &c.vfx,
        notation::model::Clip::Image(c) => &c.vfx,
        notation::model::Clip::Path(c) => &c.vfx,
        notation::model::Clip::Svg(c) => &c.vfx,
        notation::model::Clip::Chart(c) => &c.vfx,
        notation::model::Clip::Map(c) => &c.vfx,
        notation::model::Clip::Scene3d(c) => &c.vfx,
        notation::model::Clip::Caption(c) => &c.vfx,
        notation::model::Clip::Lottie(c) => &c.vfx,
        notation::model::Clip::Audio(c) => &c.vfx,
        notation::model::Clip::Nest(c) => &c.vfx,
    }
}

/// Describe one clip owner for referrer lines: the clip kind plus
/// its stable `id` or document index.
fn clip_owner(kind: &str, index: usize, id: Option<&str>) -> String {
    match id {
        Some(id) => format!("{kind} clip id {id:?}"),
        None => format!("{kind} clip {index}"),
    }
}

/// Push one normalized reference (`path` authored by `referrer`).
fn push_ref(map: &mut BTreeMap<String, Vec<String>>, path: &str, referrer: String) {
    let key = path.trim().to_string();
    if key.is_empty() {
        return;
    }
    map.entry(key).or_default().push(referrer);
}

/// Every project-relative media path the compositions reference,
/// each with its referrers in first-seen order: clip `path` values,
/// `font` declarations, grade `lut` sources (composition, lane, and
/// clip grades), box image fills, VFX matte and displacement maps,
/// and image variable defaults.
pub fn reference_map(comps: &[notation::model::Composition]) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for comp in comps {
        let cname = comp.name.clone();
        for decl in &comp.fonts {
            push_ref(
                &mut map,
                &decl.path,
                format!("composition {cname:?} font declaration"),
            );
        }
        if let Some(src) = comp.grade.lut.src.as_deref() {
            push_ref(
                &mut map,
                src,
                format!("composition {cname:?} composition grade lut"),
            );
        }
        for (lane, lane_grade) in comp.lane_grades.iter().enumerate() {
            if let Some(src) = lane_grade.grade.lut.src.as_deref() {
                push_ref(
                    &mut map,
                    src,
                    format!("composition {cname:?} lane {lane} grade lut"),
                );
            }
        }
        for var in &comp.variables {
            if let Some(notation::model::VariableDefault::Image(path)) = &var.default {
                push_ref(
                    &mut map,
                    path,
                    format!("composition {cname:?} variable {:?} default", var.id),
                );
            }
        }
        for (index, clip) in comp.clips.iter().enumerate() {
            let owner = match clip {
                notation::model::Clip::Video(c) => {
                    push_ref(
                        &mut map,
                        &c.path,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("video", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("video", index, c.id.as_deref())
                }
                notation::model::Clip::Audio(c) => {
                    push_ref(
                        &mut map,
                        &c.path,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("audio", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("audio", index, c.id.as_deref())
                }
                notation::model::Clip::Image(c) => {
                    push_ref(
                        &mut map,
                        &c.path,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("image", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("image", index, c.id.as_deref())
                }
                notation::model::Clip::Svg(c) => {
                    push_ref(
                        &mut map,
                        &c.path,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("svg", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("svg", index, c.id.as_deref())
                }
                notation::model::Clip::Lottie(c) => {
                    push_ref(
                        &mut map,
                        &c.path,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("lottie", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("lottie", index, c.id.as_deref())
                }
                notation::model::Clip::Chart(c) => {
                    if let Some(data_file) = c.data_file.as_deref() {
                        push_ref(
                            &mut map,
                            data_file,
                            format!(
                                "composition {cname:?} {}",
                                clip_owner("chart", index, c.id.as_deref())
                            ),
                        );
                    }
                    clip_owner("chart", index, c.id.as_deref())
                }
                notation::model::Clip::Map(c) => {
                    push_ref(
                        &mut map,
                        &c.pack,
                        format!(
                            "composition {cname:?} {}",
                            clip_owner("map", index, c.id.as_deref())
                        ),
                    );
                    clip_owner("map", index, c.id.as_deref())
                }
                notation::model::Clip::Scene3d(c) => clip_owner("scene3d", index, c.id.as_deref()),
                notation::model::Clip::Caption(c) => {
                    if let Some(src) = c.src.as_deref() {
                        push_ref(
                            &mut map,
                            src,
                            format!(
                                "composition {cname:?} {}",
                                clip_owner("captions", index, c.id.as_deref())
                            ),
                        );
                    }
                    if let Some(matte) = c.matte.as_deref() {
                        push_ref(
                            &mut map,
                            matte,
                            format!(
                                "composition {cname:?} {} matte",
                                clip_owner("captions", index, c.id.as_deref())
                            ),
                        );
                    }
                    clip_owner("captions", index, c.id.as_deref())
                }
                notation::model::Clip::Text(c) => clip_owner("text", index, c.id.as_deref()),
                notation::model::Clip::Box(c) => {
                    if let notation::model::FillPaint::Image { src, .. } = &c.fill_paint {
                        push_ref(
                            &mut map,
                            src,
                            format!(
                                "composition {cname:?} {} fill",
                                clip_owner("box", index, c.id.as_deref())
                            ),
                        );
                    }
                    clip_owner("box", index, c.id.as_deref())
                }
                notation::model::Clip::Flex(c) => clip_owner("flex", index, c.id.as_deref()),
                notation::model::Clip::Path(c) => clip_owner("path", index, c.id.as_deref()),
                notation::model::Clip::Nest(c) => clip_owner("nest", index, c.id.as_deref()),
            };
            let grade = clip.grade();
            if let Some(src) = grade.lut.src.as_deref() {
                push_ref(
                    &mut map,
                    src,
                    format!("composition {cname:?} {owner} grade lut"),
                );
            }
            let vfx = clip_vfx(clip);
            if let Some(luma) = &vfx.luma_matte {
                push_ref(
                    &mut map,
                    &luma.src,
                    format!("composition {cname:?} {owner} luma-matte"),
                );
            }
            if let Some(disp) = &vfx.displacement {
                if let Some(map_path) = disp.map.as_deref() {
                    push_ref(
                        &mut map,
                        map_path,
                        format!("composition {cname:?} {owner} displacement"),
                    );
                }
            }
        }
    }
    for refs in map.values_mut() {
        refs.sort();
        refs.dedup();
    }
    map
}

/// One project-relative name with `/` separators, or `None` when the
/// path escapes the project folder.
fn rel_name(folder: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(folder).ok()?;
    let mut out = String::new();
    for (i, part) in rel.components().enumerate() {
        use std::path::Component;
        match part {
            Component::Normal(s) => {
                if i > 0 {
                    out.push('/');
                }
                out.push_str(&s.to_string_lossy());
            }
            _ => return None,
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// List every managed asset under the project folder with truthful
/// used state, in name order. Hidden files and directories (names
/// starting with `.`, including the hidden SDK history directory)
/// never list: panel browsing never reads the undo record.
pub fn list_assets(folder: &Path, comps: &[notation::model::Composition]) -> Vec<AssetEntry> {
    let refs = reference_map(comps);
    let mut out = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries: Vec<_> = match std::fs::read_dir(&dir) {
            Ok(it) => it.flatten().collect(),
            Err(_) => continue,
        };
        for entry in entries {
            let name_os = entry.file_name();
            let name_lossy = name_os.to_string_lossy();
            if name_lossy.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let Some(kind) = asset_kind_of(&name_lossy) else {
                continue;
            };
            let Some(rel) = rel_name(folder, &path) else {
                continue;
            };
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let used_by = refs.get(&rel).cloned().unwrap_or_default();
            out.push(AssetEntry {
                name: rel,
                kind,
                bytes,
                used: !used_by.is_empty(),
                used_by,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Which projects the assets view covers: the served project, or
/// every sibling project holding a `composition.kdl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetScope {
    Project,
    All,
}

/// One parsed assets query: search text, type and used filters, and
/// the project scope. Key spellings are CSS-like hyphenated
/// (`all-projects`); `type` and `used` accept `all` for no filter.
#[derive(Debug, Clone)]
pub struct AssetQuery {
    /// Case-insensitive substring of the asset name; empty matches all.
    pub search: String,
    /// Kind filter; `None` lists every type.
    pub kind: Option<AssetKind>,
    /// Used-state filter; `None` lists used and unused.
    pub used: Option<bool>,
    /// Project or all-projects view.
    pub scope: AssetScope,
    /// Sibling project name for `all-projects` actions; unused by listing.
    pub project: Option<String>,
}

/// Parse the assets query string (`search`, `type`, `used`,
/// `scope`, `project`). Unknown `type` or `used` values refuse
/// naming what was written; unknown keys are ignored.
pub fn parse_assets_query(query: &str) -> Result<AssetQuery, String> {
    let mut search = String::new();
    let mut kind: Option<AssetKind> = None;
    let mut used: Option<bool> = None;
    let mut scope = AssetScope::Project;
    let mut project: Option<String> = None;
    for pair in query.split('&') {
        if pair.trim().is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => (pair.trim(), ""),
        };
        let decoded = percent_decode(value);
        match key {
            "search" => search = decoded,
            "type" => {
                let lower = decoded.to_lowercase();
                if lower == "all" || lower.is_empty() {
                    kind = None;
                } else if let Some(k) = AssetKind::parse(&lower) {
                    kind = Some(k);
                } else {
                    return Err(refused_assets(format!(
                        "unknown asset `type` {decoded:?}; pass one of `image`, `video`, `audio`, `font`, `lut`, or `all`. Nothing was listed."
                    )));
                }
            }
            "used" => {
                let lower = decoded.to_lowercase();
                if lower == "all" || lower.is_empty() {
                    used = None;
                } else if lower == "used" || lower == "true" {
                    used = Some(true);
                } else if lower == "unused" || lower == "false" {
                    used = Some(false);
                } else {
                    return Err(refused_assets(format!(
                        "unknown asset `used` filter {decoded:?}; pass `used`, `unused`, or `all`. Nothing was listed."
                    )));
                }
            }
            "scope" => {
                let lower = decoded.to_lowercase();
                if lower == "all" || lower == "all-projects" {
                    scope = AssetScope::All;
                } else if lower == "project" || lower.is_empty() {
                    scope = AssetScope::Project;
                } else {
                    return Err(refused_assets(format!(
                        "unknown asset `scope` {decoded:?}; pass `project` or `all-projects`. Nothing was listed."
                    )));
                }
            }
            // An unnamed scope stays on the served project.
            "project" if !decoded.is_empty() => {
                project = Some(decoded);
            }
            "project" => {}
            _ => {}
        }
    }
    Ok(AssetQuery {
        search,
        kind,
        used,
        scope,
        project,
    })
}

/// Minimal percent-decoding for query values (`+` reads as space).
fn percent_decode(raw: &str) -> String {
    let mut out = String::new();
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(' ');
        } else {
            out.push(bytes[i] as char);
        }
        i += 1;
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Narrow listed assets by the parsed query: the search matches a
/// case-insensitive substring of the project-relative name, and the
/// type and used filters keep their kind.
pub fn filter_assets<'a>(entries: &'a [AssetEntry], query: &AssetQuery) -> Vec<&'a AssetEntry> {
    let needle = query.search.to_lowercase();
    entries
        .iter()
        .filter(|e| {
            if !needle.is_empty() && !e.name.to_lowercase().contains(&needle) {
                return false;
            }
            if let Some(kind) = query.kind {
                if e.kind != kind {
                    return false;
                }
            }
            if let Some(used) = query.used {
                if e.used != used {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Render one project-view assets listing as JSON.
pub fn assets_json(project: &str, entries: &[&AssetEntry]) -> String {
    format!(
        "{{\"project\":\"{}\",\"scope\":\"project\",\"assets\":[{}]}}",
        json_escape(project),
        entries
            .iter()
            .map(|e| asset_entry_json(e))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// One sibling project for the all-projects view.
#[derive(Debug, Clone)]
pub struct ProjectAssets {
    /// Directory name of the project (served project included).
    pub project: String,
    /// True for every project but the served one: actions refuse.
    pub read_only: bool,
    /// The project's assets with its own used state.
    pub assets: Vec<AssetEntry>,
}

/// Render the all-projects view as JSON: every sibling project
/// holding a `composition.kdl`, each with its used state, flagging
/// which rows are read-only.
pub fn all_projects_json(projects: &[ProjectAssets], query: &AssetQuery) -> String {
    let mut rows = Vec::new();
    for p in projects {
        let kept = filter_assets(&p.assets, query);
        rows.push(format!(
            "{{\"project\":\"{}\",\"read-only\":{},\"assets\":[{}]}}",
            json_escape(&p.project),
            if p.read_only { "true" } else { "false" },
            kept.iter()
                .map(|e| asset_entry_json(e))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    format!(
        "{{\"scope\":\"all-projects\",\"projects\":[{}]}}",
        rows.join(",")
    )
}

/// Discover the projects for the all-projects view: the served
/// folder plus every sibling directory holding a `composition.kdl`,
/// in directory-name order. The served project keeps the caller's
/// effective compositions (with the persisted delta replayed);
/// sibling used state reads each sibling's validated compositions,
/// and a sibling that cannot validate lists its assets as unused
/// rather than guessing.
pub fn discover_projects(
    folder: &Path,
    served_comps: &[notation::model::Composition],
) -> Vec<ProjectAssets> {
    let mut dirs: Vec<PathBuf> = vec![folder.to_path_buf()];
    if let Some(parent) = folder.parent() {
        if let Ok(it) = std::fs::read_dir(parent) {
            for entry in it.flatten() {
                let path = entry.path();
                if !path.is_dir() || path == *folder {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if !path.join("composition.kdl").is_file() {
                    continue;
                }
                dirs.push(path);
            }
        }
    }
    dirs.sort();
    dirs.into_iter()
        .map(|dir| {
            let project = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let read_only = dir != *folder;
            let comps: Vec<notation::model::Composition> = if read_only {
                validate::validate::validate_folder(&dir).compositions
            } else {
                served_comps.to_vec()
            };
            ProjectAssets {
                project,
                read_only,
                assets: list_assets(&dir, &comps),
            }
        })
        .collect()
}

/// Directory name of the served project, for read-only checks and
/// receipts.
pub fn served_project_name(folder: &Path) -> String {
    folder
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// One parsed import request: the filesystem source path (a drop
/// lands here with its OS path) and an optional stored file name.
#[derive(Debug, Clone)]
pub struct ImportRequest {
    pub source: String,
    pub name: Option<String>,
}

/// Parse the import body (`{"source": "...", "name": "...?"}`).
/// The source is required; the name overrides the stored file name.
pub fn parse_import_body(body: &str) -> Result<ImportRequest, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_assets(format!(
            "the asset import needs a JSON body (`{{\"source\":\"<file-path>\"}}`); the body is not JSON ({e}). Nothing was copied."
        ))
    })?;
    let source = value
        .get("source")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if source.is_empty() {
        return Err(refused_assets(
            "the asset import names no `source`; pass the file path to copy into the project. Nothing was copied.".to_string(),
        ));
    }
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(ImportRequest {
        source: source.to_string(),
        name,
    })
}

/// Receipt of one applied import: the project-relative stored name,
/// its kind and size, and whether the existing file was kept.
#[derive(Debug, Clone)]
pub struct ImportReceipt {
    pub stored: String,
    pub kind: AssetKind,
    pub bytes: u64,
    pub duplicate: bool,
}

/// Render one import receipt as JSON for the import API.
pub fn import_receipt_json(receipt: &ImportReceipt) -> String {
    format!(
        "{{\"imported\":true,\"stored-name\":\"{}\",\"type\":\"{}\",\"bytes\":{},\"duplicate\":{},\"note\":\"{}\"}}",
        json_escape(&receipt.stored),
        receipt.kind.name(),
        receipt.bytes,
        if receipt.duplicate { "true" } else { "false" },
        json_escape(if receipt.duplicate {
            "the project already holds this file with identical bytes; the existing file was kept"
        } else {
            "the file was copied into the project"
        }),
    )
}

/// Keep one file stem safe for project storage: directory parts
/// stripped (a stored name never escapes the project), trimmed, and
/// non-empty.
fn safe_file_name(raw: &str) -> Option<String> {
    let base = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .unwrap_or("");
    if base.is_empty() || base == "." || base == ".." {
        return None;
    }
    Some(base.to_string())
}

/// Project-relative stored path for one kind and file name.
fn stored_path_for(kind: AssetKind, file_name: &str) -> String {
    if kind.dir().is_empty() {
        file_name.to_string()
    } else {
        format!("{}/{file_name}", kind.dir())
    }
}

/// Import one file into the project, reporting the stored
/// project-relative name. Images, video, and audio store under
/// `media/`, fonts under `fonts/`, LUT cubes at the project root.
/// An unsupported extension refuses naming the accepted types; a
/// stored name holding identical bytes keeps the existing file and
/// says so; the same name with different bytes gains a numeric
/// suffix (`clip-2.mp4`), so imports never overwrite.
pub fn import_asset(folder: &Path, req: &ImportRequest) -> Result<ImportReceipt, String> {
    let source = PathBuf::from(&req.source);
    if !source.is_file() {
        return Err(refused_assets(format!(
            "import source {:?} does not exist or is not a file; pass the file path to copy into the project. Nothing was copied.",
            req.source
        )));
    }
    let wanted = req
        .name
        .as_deref()
        .or_else(|| source.file_name().and_then(|s| s.to_str()))
        .unwrap_or("");
    let Some(file_name) = safe_file_name(wanted) else {
        return Err(refused_assets(format!(
            "import name {wanted:?} stores no file; pass a file name like `clip.mp4`. Nothing was copied."
        )));
    };
    let Some(kind) = asset_kind_of(&file_name) else {
        return Err(refused_assets(format!(
            "import {file_name:?} names no accepted asset type; accepted types are {}. Nothing was copied.",
            accepted_types_text()
        )));
    };
    let source_bytes = std::fs::read(&source).map_err(|e| {
        refused_assets(format!(
            "import source {:?} cannot be read ({e}). Nothing was copied.",
            req.source
        ))
    })?;
    let mut candidate = stored_path_for(kind, &file_name);
    let dest = folder.join(&candidate);
    if dest.is_file() {
        let existing = std::fs::read(&dest).unwrap_or_default();
        if existing == source_bytes {
            return Ok(ImportReceipt {
                stored: candidate,
                kind,
                bytes: existing.len() as u64,
                duplicate: true,
            });
        }
        let stem = Path::new(&file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("asset");
        let ext = Path::new(&file_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        let mut n = 2u32;
        loop {
            candidate = stored_path_for(kind, &format!("{stem}-{n}{ext}"));
            if !folder.join(&candidate).is_file() {
                break;
            }
            if folder.join(&candidate).is_file()
                && std::fs::read(folder.join(&candidate)).unwrap_or_default() == source_bytes
            {
                return Ok(ImportReceipt {
                    stored: candidate.clone(),
                    kind,
                    bytes: source_bytes.len() as u64,
                    duplicate: true,
                });
            }
            n += 1;
            if n > 9999 {
                return Err(refused_assets(format!(
                    "import {file_name:?} cannot find a free stored name. Nothing was copied."
                )));
            }
        }
    }
    let dest = folder.join(&candidate);
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                refused_assets(format!(
                    "import {file_name:?} cannot prepare its folder ({e}). Nothing was copied."
                ))
            })?;
        }
    }
    if !dest.starts_with(folder) {
        return Err(refused_assets(format!(
            "import name {file_name:?} escapes the project; pass a plain file name. Nothing was copied."
        )));
    }
    std::fs::write(&dest, &source_bytes).map_err(|e| {
        refused_assets(format!(
            "import {file_name:?} cannot be stored ({e}). Nothing was copied."
        ))
    })?;
    Ok(ImportReceipt {
        stored: candidate,
        kind,
        bytes: source_bytes.len() as u64,
        duplicate: false,
    })
}

/// One parsed panel action: insert, reveal, rename, or remove, with
/// the asset it targets. Key spellings are CSS-like hyphenated.
#[derive(Debug, Clone)]
pub struct AssetActionRequest {
    /// `insert`, `reveal`, `rename`, or `remove`.
    pub action: String,
    /// Project-relative asset name (`media/card.png`).
    pub asset: String,
    /// Rename target file name (`name`), insert start/duration
    /// seconds, actor recording the insert turn, and the composition
    /// insert targets.
    pub name: Option<String>,
    pub start: Option<f64>,
    pub duration: Option<f64>,
    pub actor: String,
    pub composition: Option<String>,
}

/// Parse the action body. `action` and `asset` are required; numbers
/// parse as finite seconds.
pub fn parse_action_body(body: &str) -> Result<AssetActionRequest, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        refused_assets(format!(
            "the asset action needs a JSON body (`{{\"action\":\"insert|reveal|rename|remove\",\"asset\":\"media/card.png\"}}`); the body is not JSON ({e}). Nothing was staged."
        ))
    })?;
    let action = value
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if !["insert", "reveal", "rename", "remove"].contains(&action) {
        return Err(refused_assets(format!(
            "unknown asset action {action:?}; pass `insert`, `reveal`, `rename`, or `remove`. Nothing was staged."
        )));
    }
    let asset = value
        .get("asset")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("");
    if asset.is_empty() {
        return Err(refused_assets(
            "the asset action names no `asset`; pass the project-relative name (`media/card.png`). Nothing was staged.".to_string(),
        ));
    }
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let num = |key: &str| -> Result<Option<f64>, String> {
        match value.get(key) {
            None => Ok(None),
            Some(v) if v.is_null() => Ok(None),
            Some(v) => {
                let n = v.as_f64().ok_or_else(|| {
                    refused_assets(format!(
                        "asset {key} {v} is not a number of seconds; pass a finite number. Nothing was staged."
                    ))
                })?;
                if !n.is_finite() {
                    return Err(refused_assets(format!(
                        "asset {key} {v} is not a finite number of seconds. Nothing was staged."
                    )));
                }
                Ok(Some(n))
            }
        }
    };
    let start = num("start")?;
    let duration = num("duration")?;
    let actor = value
        .get("actor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let composition = value
        .get("composition")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Ok(AssetActionRequest {
        action: action.to_string(),
        asset: asset.to_string(),
        name,
        start,
        duration,
        actor,
        composition,
    })
}

/// Reveal one asset: report its project-relative stored path. The
/// browser shell highlights the row; headless callers read the
/// receipt. A missing asset refuses naming what was asked.
pub fn reveal_asset(folder: &Path, asset: &str) -> Result<String, String> {
    let rel = asset.trim();
    if rel.is_empty() {
        return Err(refused_assets(
            "the asset reveal names no `asset`; pass the project-relative name. Nothing was staged.".to_string(),
        ));
    }
    let path = folder.join(rel);
    if !path.is_file() || !path.starts_with(folder) {
        return Err(refused_assets(format!(
            "asset {rel:?} is not in the project; pass a listed asset name. Nothing was staged."
        )));
    }
    let kind = asset_kind_of(rel).map(|k| k.name()).unwrap_or("file");
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Ok(format!(
        "{{\"revealed\":true,\"asset\":\"{}\",\"path\":\"{}\",\"type\":\"{kind}\",\"bytes\":{bytes}}}",
        json_escape(rel),
        json_escape(rel),
    ))
}

/// Rename one asset without breaking references: the asset must
/// exist, the target must be a free name with an accepted extension,
/// and a referenced asset refuses naming its referrers — unreference
/// it first, then rename.
pub fn rename_asset(
    folder: &Path,
    comps: &[notation::model::Composition],
    asset: &str,
    name: Option<&str>,
) -> Result<String, String> {
    let rel = asset.trim();
    if rel.is_empty() {
        return Err(refused_assets(
            "the asset rename names no `asset`; pass the project-relative name. Nothing was staged.".to_string(),
        ));
    }
    let from = folder.join(rel);
    if !from.is_file() || !from.starts_with(folder) {
        return Err(refused_assets(format!(
            "asset {rel:?} is not in the project; pass a listed asset name. Nothing was staged."
        )));
    }
    let wanted = name.unwrap_or("").trim();
    let Some(file_name) = safe_file_name(wanted) else {
        return Err(refused_assets(format!(
            "rename target {wanted:?} stores no file; pass a file name like `clip.mp4`. Nothing was staged."
        )));
    };
    let Some(kind) = asset_kind_of(&file_name) else {
        return Err(refused_assets(format!(
            "rename target {file_name:?} names no accepted asset type; accepted types are {}. Nothing was staged.",
            accepted_types_text()
        )));
    };
    let refs = reference_map(comps);
    if let Some(referrers) = refs.get(rel) {
        if !referrers.is_empty() {
            return Err(refused_assets(format!(
                "asset {rel:?} is still referenced by {}; unreference it first, then rename. Nothing was staged.",
                referrers
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    let candidate = stored_path_for(kind, &file_name);
    let to = folder.join(&candidate);
    if !to.starts_with(folder) {
        return Err(refused_assets(format!(
            "rename target {file_name:?} escapes the project; pass a plain file name. Nothing was staged."
        )));
    }
    if to.is_file() {
        return Err(refused_assets(format!(
            "rename target {candidate:?} already exists; pick a free name. Nothing was staged."
        )));
    }
    if let Some(parent) = to.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                refused_assets(format!(
                    "asset {rel:?} cannot prepare its rename folder ({e}). Nothing was staged."
                ))
            })?;
        }
    }
    std::fs::rename(&from, &to).map_err(|e| {
        refused_assets(format!(
            "asset {rel:?} cannot rename to {candidate:?} ({e}). Nothing was staged."
        ))
    })?;
    Ok(format!(
        "{{\"renamed\":true,\"from\":\"{}\",\"to\":\"{}\"}}",
        json_escape(rel),
        json_escape(&candidate),
    ))
}

/// Remove one asset file. A referenced asset refuses naming its
/// referrers — unreference it first, then remove. Nothing else is
/// touched.
pub fn remove_asset(
    folder: &Path,
    comps: &[notation::model::Composition],
    asset: &str,
) -> Result<String, String> {
    let rel = asset.trim();
    if rel.is_empty() {
        return Err(refused_assets(
            "the asset remove names no `asset`; pass the project-relative name. Nothing was staged.".to_string(),
        ));
    }
    let path = folder.join(rel);
    if !path.is_file() || !path.starts_with(folder) {
        return Err(refused_assets(format!(
            "asset {rel:?} is not in the project; pass a listed asset name. Nothing was staged."
        )));
    }
    let refs = reference_map(comps);
    if let Some(referrers) = refs.get(rel) {
        if !referrers.is_empty() {
            return Err(refused_assets(format!(
                "asset {rel:?} is still referenced by {}; unreference it first, then remove. Nothing was staged.",
                referrers
                    .iter()
                    .map(|r| format!("{r:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    std::fs::remove_file(&path).map_err(|e| {
        refused_assets(format!(
            "asset {rel:?} cannot be removed ({e}). Nothing was staged."
        ))
    })?;
    Ok(format!(
        "{{\"removed\":true,\"asset\":\"{}\"}}",
        json_escape(rel),
    ))
}

/// Insert one asset on the timeline as exactly one patch turn: an
/// `image` clip for images (and SVGs), a `video` clip for video, an
/// `audio` clip for audio. Fonts and LUTs refuse naming how they
/// join instead: fonts through `font` declarations in the notation,
/// LUT cubes through a grade `lut` source. The asset must exist in
/// the project; the window defaults to the composition start and
/// length.
pub fn insert_asset(
    folder: &Path,
    comps: &[notation::model::Composition],
    req: &AssetActionRequest,
) -> Result<String, String> {
    let rel = req.asset.trim();
    if !folder.join(rel).is_file() || !folder.join(rel).starts_with(folder) {
        return Err(refused_assets(format!(
            "asset {rel:?} is not in the project; import it first, then insert. Nothing was staged."
        )));
    }
    let Some(kind) = asset_kind_of(rel) else {
        return Err(refused_assets(format!(
            "asset {rel:?} names no accepted asset type; accepted types are {}. Nothing was staged.",
            accepted_types_text()
        )));
    };
    match kind {
        AssetKind::Image | AssetKind::Video | AssetKind::Audio => {}
        AssetKind::Font => {
            return Err(refused_assets(format!(
                "asset {rel:?} is a font; fonts join through `font` declarations in the notation, so the panel cannot insert it. Nothing was staged."
            )));
        }
        AssetKind::Lut => {
            return Err(refused_assets(format!(
                "asset {rel:?} is a LUT cube; LUTs join through a grade `lut` source, so the panel cannot insert it. Nothing was staged."
            )));
        }
    };
    let target = match req.composition.as_deref() {
        None => comps.first().ok_or_else(|| {
            refused_assets(
                "the project holds no composition; insert names nothing. Nothing was staged.".to_string(),
            )
        })?,
        Some(want) => comps.iter().find(|c| c.name == want).ok_or_else(|| {
            refused_assets(format!(
                "insert names unknown composition {want:?}: the project holds {}; pass one of those. Nothing was staged.",
                comps
                    .iter()
                    .map(|c| format!("{:?}", c.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?,
    };
    let start = req.start.unwrap_or(0.0);
    if start < 0.0 {
        return Err(refused_assets(format!(
            "insert `start` {start} runs before the composition; pass a start at or after 0. Nothing was staged."
        )));
    }
    let duration = match req.duration {
        Some(d) => {
            if d <= 0.0 {
                return Err(refused_assets(format!(
                    "insert `duration` {d} runs no time; pass a positive duration. Nothing was staged."
                )));
            }
            d
        }
        None => {
            if !target.duration.is_finite() || target.duration <= 0.0 {
                return Err(refused_assets(format!(
                    "composition {:?} carries no positive duration; pass an explicit `duration`. Nothing was staged.",
                    target.name
                )));
            }
            (target.duration - start).max(0.001)
        }
    };
    let element = match kind {
        AssetKind::Image => sdk::NewElement::Image {
            id: None,
            label: None,
            path: rel.to_string(),
            start,
            duration,
        },
        AssetKind::Video => sdk::NewElement::Video {
            id: None,
            label: None,
            path: rel.to_string(),
            start,
            duration,
        },
        AssetKind::Audio => sdk::NewElement::Audio {
            id: None,
            label: None,
            path: rel.to_string(),
            start,
            duration,
        },
        // Fonts and LUTs return above, naming how they join instead;
        // this arm only stands if that guard ever moves.
        AssetKind::Font | AssetKind::Lut => {
            return Err(refused_assets(format!(
                "asset {rel:?} cannot insert on the timeline; fonts join through `font` declarations and LUTs through a grade `lut` source. Nothing was staged."
            )));
        }
    };
    let actor = if req.actor.trim().is_empty() {
        "maker".to_string()
    } else {
        req.actor.trim().to_string()
    };
    let edit = crate::studio::StudioEdit {
        actor,
        source: crate::studio::SaveSource::Visual,
        ops: vec![sdk::EditOp::AddElement {
            element,
            position: None,
        }],
    };
    match crate::studio::apply_studio_edit(folder, Some(&target.name), &edit) {
        Ok(receipt) => Ok(format!(
            "{{\"inserted\":true,\"asset\":\"{}\",\"composition\":\"{}\",\"turn\":{},\"summaries\":[{}]}}",
            json_escape(rel),
            json_escape(&target.name),
            receipt.turn,
            receipt
                .summaries
                .iter()
                .map(|s| format!("\"{}\"", json_escape(s)))
                .collect::<Vec<_>>()
                .join(",")
        )),
        Err(message) => Err(message),
    }
}
