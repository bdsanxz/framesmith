//! Item manifest schema (Sprint 19, T001 / CAP-001).
//!
//! A manifest is a JSON object shaped like:
//!
//! ```json
//! {
//!   "name": "greeting-card",
//!   "type": "block",
//!   "title": "Greeting Card",
//!   "description": "A short original title card.",
//!   "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
//!   "width": 1280,
//!   "height": 720,
//!   "duration": 3,
//!   "tags": ["titles"],
//!   "author": "Framesmith",
//!   "license": "MIT",
//!   "minCliVersion": "0.1.0",
//!   "deprecated": false,
//!   "registryDependencies": [],
//!   "preview": "preview.png",
//!   "relatedSkill": "video",
//!   "params": [],
//!   "variables": []
//! }
//! ```
//!
//! Required fields are `name`, `type`, `title`, `description`, and
//! `files` (non-empty, every entry carrying a `src` and a `target`).
//! `width`, `height`, and `duration` are required for blocks and
//! examples and forbidden for components. Every optional field reads
//! back truthfully when present; unknown fields are ignored so older
//! readers keep listing newer manifests. Validation is fail-closed:
//! every violation is reported with its field name, and `params` /
//! `variables` entries must at least be objects (their deeper shape
//! is an install-time concern for later slices).

use serde_json::Value;

/// The manifest file name inside every item directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Item kinds: a `block` is a complete composition, a `component` is
/// a snippet, and an `example` is a starter project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Block,
    Component,
    Example,
}

impl ItemType {
    /// The manifest spelling of the kind.
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Block => "block",
            ItemType::Component => "component",
            ItemType::Example => "example",
        }
    }

    /// Parse the manifest spelling; `None` for anything else.
    pub fn from_str(s: &str) -> Option<ItemType> {
        match s {
            "block" => Some(ItemType::Block),
            "component" => Some(ItemType::Component),
            "example" => Some(ItemType::Example),
            _ => None,
        }
    }
}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The voice-or-sfx role of an adopted audio file (Sprint 19, T003 /
/// CAP-003): every registry file that is adopted audio carries one of
/// these labels so the installer can echo it in the resolved order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRole {
    Voice,
    Sfx,
}

impl AudioRole {
    /// The manifest spelling of the role.
    pub fn as_str(self) -> &'static str {
        match self {
            AudioRole::Voice => "voice",
            AudioRole::Sfx => "sfx",
        }
    }

    /// Parse the manifest spelling; `None` for anything else.
    pub fn from_str(s: &str) -> Option<AudioRole> {
        match s {
            "voice" => Some(AudioRole::Voice),
            "sfx" => Some(AudioRole::Sfx),
            _ => None,
        }
    }
}

impl std::fmt::Display for AudioRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One file shipped by an item: the registry-relative `src` and the
/// project-relative `target` it installs to, plus the optional
/// voice-or-sfx role the installer echoes for adopted audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemFile {
    pub src: String,
    pub target: String,
    pub audio: Option<AudioRole>,
}

/// A validated item manifest: required fields plus every optional
/// field exactly as written. `params` and `variables` stay as JSON
/// values so listing reads them truthfully without inventing a
/// parameter schema this slice does not own.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemManifest {
    pub name: String,
    pub item_type: ItemType,
    pub title: String,
    pub description: String,
    pub files: Vec<ItemFile>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub duration: Option<f64>,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub min_cli_version: Option<String>,
    pub deprecated: bool,
    pub registry_dependencies: Vec<String>,
    pub preview: Option<String>,
    pub related_skill: Option<String>,
    pub params: Vec<Value>,
    pub variables: Vec<Value>,
}

/// One schema failure: the failing field (for example `"width"` or
/// `"files[0].target"`) plus a human message. The special field
/// `"manifest"` names a manifest that is not a JSON object at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "field {:?}: {}", self.field, self.message)
    }
}

fn err(field: &str, message: String) -> ManifestError {
    ManifestError {
        field: field.to_string(),
        message,
    }
}

/// Item-name rule (same shape as the CLI project-name rule): 1–64
/// characters, ASCII letters/digits/`-`/`_`, starting with a letter
/// or digit.
fn check_name(name: &str) -> Option<String> {
    if name.is_empty() || name.len() > 64 {
        return Some("name must be 1 to 64 characters long".to_string());
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return Some("name must start with a letter or digit".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some("name may only contain letters, digits, `-` and `_`".to_string());
    }
    None
}

fn required_string(
    obj: &[(String, Value)],
    field: &str,
    errors: &mut Vec<ManifestError>,
) -> Option<String> {
    match obj.iter().find(|(k, _)| k == field) {
        None => {
            errors.push(err(field, format!("{field} is required")));
            None
        }
        Some((_, Value::String(s))) if !s.trim().is_empty() => Some(s.clone()),
        Some((_, Value::String(_))) => {
            errors.push(err(field, format!("{field} must not be empty")));
            None
        }
        Some(_) => {
            errors.push(err(field, format!("{field} must be a string")));
            None
        }
    }
}

fn optional_string(
    obj: &[(String, Value)],
    field: &str,
    errors: &mut Vec<ManifestError>,
) -> Option<String> {
    match obj.iter().find(|(k, _)| k == field) {
        None => None,
        Some((_, Value::String(s))) if !s.trim().is_empty() => Some(s.clone()),
        Some((_, Value::String(_))) => {
            errors.push(err(
                field,
                format!("{field} must not be empty when present"),
            ));
            None
        }
        Some(_) => {
            errors.push(err(field, format!("{field} must be a string when present")));
            None
        }
    }
}

fn string_list(
    obj: &[(String, Value)],
    field: &str,
    errors: &mut Vec<ManifestError>,
) -> Option<Vec<String>> {
    match obj.iter().find(|(k, _)| k == field) {
        None => Some(Vec::new()),
        Some((_, Value::Array(items))) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, v) in items.iter().enumerate() {
                match v {
                    Value::String(s) if !s.trim().is_empty() => out.push(s.clone()),
                    _ => {
                        errors.push(err(
                            field,
                            format!("{field}[{i}] must be a non-empty string"),
                        ));
                    }
                }
            }
            Some(out)
        }
        Some(_) => {
            errors.push(err(
                field,
                format!("{field} must be an array of strings when present"),
            ));
            None
        }
    }
}

fn object_list(
    obj: &[(String, Value)],
    field: &str,
    errors: &mut Vec<ManifestError>,
) -> Option<Vec<Value>> {
    match obj.iter().find(|(k, _)| k == field) {
        None => Some(Vec::new()),
        Some((_, Value::Array(items))) => {
            for (i, v) in items.iter().enumerate() {
                if !v.is_object() {
                    errors.push(err(field, format!("{field}[{i}] must be an object")));
                }
            }
            Some(items.clone())
        }
        Some(_) => {
            errors.push(err(
                field,
                format!("{field} must be an array of objects when present"),
            ));
            None
        }
    }
}

fn dimension(
    obj: &[(String, Value)],
    field: &str,
    kind: ItemType,
    errors: &mut Vec<ManifestError>,
) -> Option<u64> {
    let present = obj.iter().find(|(k, _)| k == field).map(|(_, v)| v);
    match (kind, present) {
        (ItemType::Component, Some(_)) => {
            errors.push(err(
                field,
                format!("{field} is for blocks and examples; components never carry it"),
            ));
            None
        }
        (ItemType::Component, None) => None,
        (_, None) => {
            errors.push(err(
                field,
                format!("{field} is required for blocks and examples"),
            ));
            None
        }
        (_, Some(Value::Number(n))) => match n.as_u64() {
            Some(v) if v > 0 => Some(v),
            _ => {
                errors.push(err(
                    field,
                    format!("{field} must be a positive whole number of pixels"),
                ));
                None
            }
        },
        (_, Some(_)) => {
            errors.push(err(
                field,
                format!("{field} must be a positive whole number of pixels"),
            ));
            None
        }
    }
}

fn item_duration(
    obj: &[(String, Value)],
    kind: ItemType,
    errors: &mut Vec<ManifestError>,
) -> Option<f64> {
    let present = obj.iter().find(|(k, _)| k == "duration").map(|(_, v)| v);
    match (kind, present) {
        (ItemType::Component, Some(_)) => {
            errors.push(err(
                "duration",
                "duration is for blocks and examples; components never carry it".to_string(),
            ));
            None
        }
        (ItemType::Component, None) => None,
        (_, None) => {
            errors.push(err(
                "duration",
                "duration is required for blocks and examples".to_string(),
            ));
            None
        }
        (_, Some(Value::Number(n))) => match n.as_f64() {
            Some(v) if v.is_finite() && v > 0.0 => Some(v),
            _ => {
                errors.push(err(
                    "duration",
                    "duration must be a positive number of seconds".to_string(),
                ));
                None
            }
        },
        (_, Some(_)) => {
            errors.push(err(
                "duration",
                "duration must be a positive number of seconds".to_string(),
            ));
            None
        }
    }
}

fn item_files(obj: &[(String, Value)], errors: &mut Vec<ManifestError>) -> Option<Vec<ItemFile>> {
    let items = match obj.iter().find(|(k, _)| k == "files") {
        None => {
            errors.push(err("files", "files is required".to_string()));
            return None;
        }
        Some((_, Value::Array(items))) => items,
        Some(_) => {
            errors.push(err("files", "files must be an array".to_string()));
            return None;
        }
    };
    if items.is_empty() {
        errors.push(err(
            "files",
            "files must list at least one file".to_string(),
        ));
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    let mut ok = true;
    for (i, v) in items.iter().enumerate() {
        let entry = match v.as_object() {
            Some(e) => e,
            None => {
                errors.push(err("files", format!("files[{i}] must be an object")));
                ok = false;
                continue;
            }
        };
        let get = |key: &str| -> Option<String> {
            match entry.get(key) {
                Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
                _ => None,
            }
        };
        let src = get("src");
        let target = get("target");
        if src.is_none() {
            errors.push(err(
                "files",
                format!("files[{i}].src is required and must not be empty"),
            ));
            ok = false;
        }
        if target.is_none() {
            errors.push(err(
                "files",
                format!("files[{i}].target is required and must not be empty"),
            ));
            ok = false;
        }
        // The optional voice-or-sfx role of adopted audio: an unknown
        // spelling is refused here (fail-closed, never guessed) so a
        // later install never echoes a label the manifest did not mean.
        let audio = match entry.get("audio") {
            None => None,
            Some(Value::String(s)) => match AudioRole::from_str(s) {
                Some(role) => Some(role),
                None => {
                    errors.push(err(
                        "files",
                        format!(
                            "files[{i}].audio must be \"voice\" or \"sfx\" when present, found {s:?}"
                        ),
                    ));
                    ok = false;
                    None
                }
            },
            Some(_) => {
                errors.push(err(
                    "files",
                    format!("files[{i}].audio must be \"voice\" or \"sfx\" when present"),
                ));
                ok = false;
                None
            }
        };
        if let (Some(src), Some(target)) = (src, target) {
            out.push(ItemFile { src, target, audio });
        }
    }
    if ok {
        Some(out)
    } else {
        None
    }
}

fn validate_object(fields: &[(String, Value)]) -> Result<ItemManifest, Vec<ManifestError>> {
    let mut errors: Vec<ManifestError> = Vec::new();

    let name = required_string(fields, "name", &mut errors);
    if let Some(ref n) = name {
        if let Some(rule) = check_name(n) {
            errors.push(err("name", rule));
        }
    }
    let item_type = match fields.iter().find(|(k, _)| k == "type") {
        None => {
            errors.push(err("type", "type is required".to_string()));
            None
        }
        Some((_, Value::String(s))) => match ItemType::from_str(s) {
            Some(t) => Some(t),
            None => {
                errors.push(err(
                    "type",
                    "type must be one of \"block\", \"component\", or \"example\"".to_string(),
                ));
                None
            }
        },
        Some(_) => {
            errors.push(err("type", "type must be a string".to_string()));
            None
        }
    };
    let title = required_string(fields, "title", &mut errors);
    let description = required_string(fields, "description", &mut errors);
    let files = item_files(fields, &mut errors);

    // Dimensions read against the declared kind; an unknown kind
    // still reports its own `type` failure above while dimensions
    // fall back to the block/example requirement so one bad `type`
    // does not silence three more precise failures.
    let kind = item_type.unwrap_or(ItemType::Block);
    let width = dimension(fields, "width", kind, &mut errors);
    let height = dimension(fields, "height", kind, &mut errors);
    let duration = item_duration(fields, kind, &mut errors);

    let tags = string_list(fields, "tags", &mut errors);
    let author = optional_string(fields, "author", &mut errors);
    let license = optional_string(fields, "license", &mut errors);
    let min_cli_version = optional_string(fields, "minCliVersion", &mut errors);
    let deprecated = match fields.iter().find(|(k, _)| k == "deprecated") {
        None => Some(false),
        Some((_, Value::Bool(b))) => Some(*b),
        Some(_) => {
            errors.push(err(
                "deprecated",
                "deprecated must be true or false when present".to_string(),
            ));
            None
        }
    };
    let registry_dependencies = string_list(fields, "registryDependencies", &mut errors);
    let preview = optional_string(fields, "preview", &mut errors);
    let related_skill = optional_string(fields, "relatedSkill", &mut errors);
    let params = object_list(fields, "params", &mut errors);
    let variables = object_list(fields, "variables", &mut errors);

    // A manifest whose `type` failed still reports that failure;
    // without a known kind no honest manifest can be built.
    let (
        Some(name),
        Some(item_type),
        Some(title),
        Some(description),
        Some(files),
        Some(tags),
        Some(deprecated),
        Some(registry_dependencies),
        Some(params),
        Some(variables),
    ) = (
        name,
        item_type,
        title,
        description,
        files,
        tags,
        deprecated,
        registry_dependencies,
        params,
        variables,
    )
    else {
        return Err(errors);
    };
    // Dimensions are `Some` exactly when they validated; a `None`
    // beside an empty error list means "forbidden and absent" on a
    // component, which is the honest shape.
    if matches!(item_type, ItemType::Block | ItemType::Example)
        && (width.is_none() || height.is_none() || duration.is_none())
    {
        return Err(errors);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(ItemManifest {
        name,
        item_type,
        title,
        description,
        files,
        width,
        height,
        duration,
        tags,
        author,
        license,
        min_cli_version,
        deprecated,
        registry_dependencies,
        preview,
        related_skill,
        params,
        variables,
    })
}

/// Parse one manifest document. `Ok` carries the validated manifest;
/// `Err` carries every failing field, so one listing names them all.
pub fn parse_manifest_str(text: &str) -> Result<ItemManifest, Vec<ManifestError>> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| vec![err("manifest", format!("manifest is not valid JSON: {e}"))])?;
    match value {
        Value::Object(map) => {
            let fields: Vec<(String, Value)> = map.into_iter().collect();
            validate_object(&fields)
        }
        _ => Err(vec![err(
            "manifest",
            "manifest must be a JSON object".to_string(),
        )]),
    }
}

/// The parsed `name` when the document is an object carrying a string
/// name — the identifier broken-item listings prefer.
pub fn peek_name(text: &str) -> Option<String> {
    let value: Value = serde_json::from_str(text).ok()?;
    value
        .as_object()?
        .get("name")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = r#"{
        "name": "greeting-card",
        "type": "block",
        "title": "Greeting Card",
        "description": "A short original title card.",
        "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
        "width": 1280,
        "height": 720,
        "duration": 3
    }"#;

    #[test]
    fn healthy_block_reads_back() {
        let m = parse_manifest_str(BLOCK).expect("healthy block parses");
        assert_eq!(m.name, "greeting-card");
        assert_eq!(m.item_type, ItemType::Block);
        assert_eq!(m.width, Some(1280));
        assert_eq!(m.height, Some(720));
        assert_eq!(m.duration, Some(3.0));
        assert_eq!(m.deprecated, false);
        assert!(m.tags.is_empty());
    }

    #[test]
    fn component_with_dimensions_fails_naming_each_field() {
        let text = r#"{
            "name": "soft-box", "type": "component",
            "title": "Soft Box", "description": "A snippet.",
            "files": [{"src": "snippet.kdl", "target": "snippet.kdl"}],
            "width": 100, "height": 50, "duration": 2
        }"#;
        let errors = parse_manifest_str(text).expect_err("component dims refuse");
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert!(fields.contains(&"width"), "names width: {fields:?}");
        assert!(fields.contains(&"height"), "names height: {fields:?}");
        assert!(fields.contains(&"duration"), "names duration: {fields:?}");
    }

    #[test]
    fn bad_name_and_type_name_their_fields() {
        let text = r#"{
            "name": "has space", "type": "widget",
            "title": "T", "description": "D",
            "files": [{"src": "a", "target": "b"}],
            "width": 64, "height": 64, "duration": 1
        }"#;
        let errors = parse_manifest_str(text).expect_err("bad name+type refuse");
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert!(fields.contains(&"name"), "names name: {fields:?}");
        assert!(fields.contains(&"type"), "names type: {fields:?}");
    }

    #[test]
    fn fractional_dimensions_refuse() {
        let text = BLOCK.replace("\"width\": 1280", "\"width\": 1280.5");
        let errors = parse_manifest_str(&text).expect_err("fractional width refuses");
        assert!(errors.iter().any(|e| e.field == "width"));
    }
}
