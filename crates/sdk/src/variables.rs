//! Variable declare, validate, scan, and override deltas (Sprint 15, T004 / CAP-005).
//!
//! An agent declares variables on the open composition from T001
//! ([`SdkSession::open`](crate::SdkSession::open)), validates the set
//! with every offender reported by rule and location, scans usage
//! listing every `var(--id)` reference, and applies override deltas
//! without touching declared defaults. Undeclared variables in
//! overrides are reported, never silently created; a failed validation
//! blocks persistence with nothing partial saved.
//!
//! Persistence boundary: declarations and override deltas stage in
//! memory only. [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
//! writes two SDK-owned delta files under the hidden project
//! directory — `<project>/.framesmith/sdk-variables.json` and
//! `<project>/.framesmith/sdk-variable-overrides.json`
//! ([`sdk_variables_path_for`], [`sdk_variable_overrides_path_for`]) —
//! and nothing else: composition sources stay byte-identical, no
//! render output is produced, and render, check, and validation never
//! read either file. Reopening replays both onto the validated
//! composition, so a declared variable and its overrides return.
//!
//! Refusal boundary: declare and override checks run before anything
//! stages. A refused call names a reason code (`E_SDK_*`) with the
//! cause and the fix, and leaves staged state, composition sources,
//! and history exactly unchanged.
//!
//! Naming boundary: style attributes stay CSS-like hyphenated end to
//! end (`font-family`, `border-color`), including in scanned attribute
//! names, which pass through as authored. A `font` default or override
//! is one comma-separated try-order list, preserved as authored and
//! never resolved or substituted here: unknown families behave exactly
//! as `check` treats them (syntax-checked here, fail-closed at render
//! time), so declaring never guesses a face silently.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use notation::model::{VariableDecl, VariableDefault, VariableType};
use validate::validate::{OverrideValue, VariableOverrides};

/// Reason code: a declaration or override value breaks its rule.
pub const E_SDK_BAD_VARIABLE: &str = "E_SDK_BAD_VARIABLE";
/// Reason code: an override names no declared variable.
pub const E_SDK_UNKNOWN_VARIABLE: &str = "E_SDK_UNKNOWN_VARIABLE";
/// Reason code: failed variable validation blocks persistence.
pub const E_SDK_INVALID_VARIABLES: &str = "E_SDK_INVALID_VARIABLES";

/// SDK-owned variable declaration delta file name inside the hidden
/// project directory. Render, check, and validation never read it;
/// only [`crate::SdkSession::open`] replays it and
/// [`crate::SdkSession::persist_variables`] writes it.
pub const SDK_VARIABLES_FILE: &str = "sdk-variables.json";

/// SDK-owned variable override delta file name inside the hidden
/// project directory, with the same read/write boundary as
/// [`SDK_VARIABLES_FILE`].
pub const SDK_VARIABLE_OVERRIDES_FILE: &str = "sdk-variable-overrides.json";

/// Version of both persisted variable deltas. A file carrying another
/// version refuses the open naming the file, never guessed.
pub const SDK_VARIABLES_VERSION: u32 = 1;

/// Hidden variable declaration delta path for one project folder.
pub fn sdk_variables_path_for(folder: &Path) -> PathBuf {
    folder
        .join(crate::HISTORY_DIR_NAME)
        .join(SDK_VARIABLES_FILE)
}

/// Hidden variable override delta path for one project folder.
pub fn sdk_variable_overrides_path_for(folder: &Path) -> PathBuf {
    folder
        .join(crate::HISTORY_DIR_NAME)
        .join(SDK_VARIABLE_OVERRIDES_FILE)
}

/// Why a variable call refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableRefusalKind {
    /// The session is read-only (`persist: false`): nothing may stage.
    ReadOnly,
    /// The session is a dry-run preview: stages, never persists.
    Preview,
    /// A declaration or override value breaks its rule.
    BadVariable,
    /// An override names no declared variable.
    UnknownVariable,
    /// Failed validation blocks persistence.
    Invalid,
    /// A hidden delta file cannot be written.
    Unwritable,
}

/// A refused variable call: the kind, the reason code, and a message
/// naming the cause with the fix. Nothing starts on refusal: no
/// staged change, no composition change, no history change.
#[derive(Debug, Clone)]
pub struct VariableRefusal {
    pub kind: VariableRefusalKind,
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for VariableRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

fn refused(kind: VariableRefusalKind, code: &'static str, message: String) -> VariableRefusal {
    VariableRefusal {
        kind,
        code,
        message,
    }
}

/// The shared blocked-persistence refusal: validation reports
/// `offenders`, so nothing was saved — no partial save.
fn invalid_variables_refusal(offenders: &[VariableOffender]) -> VariableRefusal {
    let mut message = format!(
        "sdk refused: variable validation reports {} offender(s); persistence is blocked with nothing saved.",
        offenders.len()
    );
    for o in offenders {
        message.push_str(&format!(
            "\n{} {}:{}:{} {}",
            o.rule, o.file, o.line, o.column, o.message
        ));
    }
    refused(
        VariableRefusalKind::Invalid,
        E_SDK_INVALID_VARIABLES,
        message,
    )
}

/// One variable declaration authored through the SDK (Sprint 15,
/// T004): the `id`, the `type` (one of the seven declared types), the
/// `default` as authored text (a font default is one comma-separated
/// try-order list, an image default one project-relative path), an
/// optional editor `label`, and (`enum` only) `options` as one
/// comma-separated list. Every field validates exactly like `check`
/// validates a `variable` element.
#[derive(Debug, Clone)]
pub struct VariableInput {
    pub id: String,
    pub var_type: String,
    pub default: Option<String>,
    pub label: Option<String>,
    pub options: Option<String>,
}

/// One declared variable read back after staging: the id, the
/// canonical type name, the canonical default text when the
/// declaration carries one, and the editor label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredVariable {
    pub id: String,
    pub var_type: String,
    pub default: Option<String>,
    pub label: Option<String>,
}

/// One variable offender: the check rule (`E_*` code) with the file,
/// line, and column owning it, plus the message and the fix — the
/// same shape `check` prints, so the maker fixes the same problem the
/// same way. Offenders on staged (not yet persisted) declarations
/// report file `<sdk-staged>` (declarations) or
/// `<sdk-staged-overrides>` (override values) at line 0, column 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableOffender {
    pub rule: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub fix: String,
}

/// One `var(--id)` reference: the variable id with the file, line,
/// and column of the call plus the hyphenated attribute holding it
/// (`value`, `font-family`, ...), as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableUse {
    pub variable: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub attribute: String,
}

/// What [`crate::SdkSession::apply_variable_overrides`] staged: the
/// applied ids in sorted order. Undeclared keys never reach a report:
/// they refuse the whole delta first (see
/// [`E_SDK_UNKNOWN_VARIABLE`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideDeltaReport {
    pub applied: Vec<String>,
}

/// Marker of the strict unused-variable diagnostic (`check`
/// CAP-004): a pre-existing disk declaration with no binding.
/// Panels list it as an unused warning, never a blocker.
pub const STRICT_UNUSED_MARKER: &str = "`--strict-variables` refuses unused variable";

/// Marker of the strict default-less diagnostic (`check`
/// CAP-004): a pre-existing disk declaration with no value from
/// any source. Panels list it as a warning, never a blocker.
pub const STRICT_DEFAULTLESS_MARKER: &str = "`--strict-variables` refuses default-less variable";

/// Whether one hard offender is pre-existing disk variable
/// health (see markers): tolerated as a listed warning, never a
/// persist blocker. Staged files never carry these markers.
fn is_disk_health_advisory(offender: &VariableOffender) -> bool {
    offender.file != STAGED_DECLARATIONS_FILE
        && offender.file != STAGED_OVERRIDES_FILE
        && (offender.message.starts_with(STRICT_UNUSED_MARKER)
            || offender.message.starts_with(STRICT_DEFAULTLESS_MARKER))
}

/// Split hard offenders into persist blockers and tolerated disk
/// health: failed rules, duplicates, and unknown or mistyped
/// staged values block writing nothing; pre-existing disk
/// unused and default-less diagnostics persist as warnings —
/// callers list them but cannot fix them through the staged
/// change (their declarations live in the KDL sources), so
/// blocking the staged change on them would deadlock panel
/// flows.
fn partition_hard_offenders(
    hard: Vec<VariableOffender>,
) -> (Vec<VariableOffender>, Vec<VariableOffender>) {
    let mut blocking = Vec::new();
    let mut advisory = Vec::new();
    for offender in hard {
        if is_disk_health_advisory(&offender) {
            advisory.push(offender);
        } else {
            blocking.push(offender);
        }
    }
    advisory.sort_by(|a, b| {
        (&a.file, a.line, a.column, &a.rule, &a.message)
            .cmp(&(&b.file, b.line, b.column, &b.rule, &b.message))
    });
    (blocking, advisory)
}

/// What [`crate::SdkSession::persist_variables_with_warnings`] wrote
/// and warned: the strict write report plus the advisory (soft)
/// offenders the caller lists instead of blocking on — an unused
/// declaration persists as a warning, never a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariablePersistOutcome {
    /// Staged declarations and overrides written by this persist.
    pub report: VariablePersistReport,
    /// Advisory offenders (staged-but-never-bound and
    /// default-less-without-value), sorted for display.
    pub warnings: Vec<VariableOffender>,
}

/// What [`crate::SdkSession::persist_variables`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariablePersistReport {
    /// Staged declarations written by this persist.
    pub declarations: usize,
    /// Staged overrides written by this persist.
    pub overrides: usize,
    /// Delta paths written, in write order (empty when nothing was
    /// staged: persisting a clean stage writes nothing).
    pub paths: Vec<PathBuf>,
}

/// One SDK-staged variable declaration: the persisted record form.
/// This is the delta file row: id, type name, authored default text,
/// editor label, and (`enum` only) options entries.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VariableRecord {
    pub id: String,
    pub var_type: String,
    pub default: Option<String>,
    pub label: Option<String>,
    pub options: Option<Vec<String>>,
}

/// One staged declaration with whether it reached the delta file.
#[derive(Debug, Clone)]
pub struct StagedVariable {
    pub raw: VariableRecord,
    pub persisted: bool,
}

/// The persisted declaration delta: versioned records only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct VariablesFile {
    version: u32,
    variables: Vec<VariableRecord>,
}

/// The persisted override delta: versioned caller values as JSON
/// scalars (string, finite number, boolean), converted back to
/// [`OverrideValue`] on replay.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct VariableOverridesFile {
    version: u32,
    overrides: BTreeMap<String, serde_json::Value>,
}

/// Variable `id` syntax, exactly like `check` (Sprint 9 variables,
/// CAP-001): starts with an ASCII letter, then ASCII letters, digits,
/// dashes, or underscores.
pub(crate) fn valid_variable_id(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Split one comma-separated list exactly like `check`: entries trim
/// whitespace and empties drop, so a font try-order list keeps its
/// authored order with no gaps.
fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// Canonical text of one typed default: strings verbatim, numbers in
/// plain form, colors lowercase `#rrggbb`, booleans bare, enums
/// verbatim, fonts joined in try order, images verbatim.
fn canonical_default_text(default: &VariableDefault) -> String {
    match default {
        VariableDefault::Str(s) => s.clone(),
        VariableDefault::Num(x) => x.to_string(),
        VariableDefault::Color(c) => format!("#{}", c.to_hex()),
        VariableDefault::Bool(b) => b.to_string(),
        VariableDefault::Enum(s) => s.clone(),
        VariableDefault::Font(names) => names.join(", "),
        VariableDefault::Image(path) => path.clone(),
    }
}

/// One readable-file check mirroring `check`: the path exists and one
/// byte reads.
fn path_readable(abs: &Path) -> bool {
    if !abs.is_file() {
        return false;
    }
    std::fs::File::open(abs)
        .and_then(|mut f| {
            use std::io::Read;
            f.read(&mut [0u8; 1]).map(|_| ())
        })
        .is_ok()
}

/// Type-check one authored default against its declaration, exactly
/// like `check` type-checks a `variable` default: strings take any
/// text, numbers take finite decimals, colors take `#rgb`/`#rrggbb`,
/// booleans take `true`/`false`, enums take one `options` entry
/// exactly, fonts take a non-empty comma try-order list (order
/// preserved, never resolved), images take a project-relative path
/// that exists and reads. Returns the typed default or the
/// (`rule`, `message`, `fix`) triple for the refusal.
fn typed_default(
    folder: &Path,
    who: &str,
    var_type: VariableType,
    text: &str,
    options: Option<&[String]>,
) -> Result<VariableDefault, (String, String, String)> {
    const BAD: &str = "E_BAD_VARIABLE";
    match var_type {
        VariableType::String => Ok(VariableDefault::Str(text.to_string())),
        VariableType::Number => match text.trim().parse::<f64>() {
            Ok(x) if x.is_finite() => Ok(VariableDefault::Num(x)),
            _ => Err((
                BAD.to_string(),
                format!(
                    "variable {who:?} default {text:?} is not a number; a `number` default holds decimals like `default=\"1.5\"`"
                ),
                "set `default` to a finite number (for example `default=\"1.5\"`), or omit it for a default-less declaration".to_string(),
            )),
        },
        VariableType::Color => match notation::model::parse_color(text) {
            Some(c) => Ok(VariableDefault::Color(c)),
            None => Err((
                BAD.to_string(),
                format!(
                    "variable {who:?} default {text:?} is not a `#rgb` or `#rrggbb` color"
                ),
                "set `default=\"#rrggbb\"` (for example `default=\"#ff3366\"`), or omit it for a default-less declaration".to_string(),
            )),
        },
        VariableType::Boolean => match text {
            "true" => Ok(VariableDefault::Bool(true)),
            "false" => Ok(VariableDefault::Bool(false)),
            _ => Err((
                BAD.to_string(),
                format!(
                    "variable {who:?} default {text:?} is not a boolean; accepted spellings are `true` and `false`"
                ),
                "set `default=\"true\"` or `default=\"false\"`, or omit it for a default-less declaration".to_string(),
            )),
        },
        VariableType::Enum => match options {
            None => Err((
                BAD.to_string(),
                format!("variable {who:?} has no `options` to draw its default from"),
                "declare `options=\"<first>, <second>\"` with at least two comma-separated entries".to_string(),
            )),
            Some(entries) if entries.iter().any(|e| e == text) => {
                Ok(VariableDefault::Enum(text.to_string()))
            }
            Some(entries) => {
                let list = entries
                    .iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                Err((
                    BAD.to_string(),
                    format!(
                        "variable {who:?} default {text:?} is not one of its `options`: {list}"
                    ),
                    "set `default` to one of the declared `options`, or add the value to `options`".to_string(),
                ))
            }
        },
        VariableType::Font => {
            let names = split_list(text);
            if names.is_empty() {
                Err((
                    BAD.to_string(),
                    format!("variable {who:?} default font list names no family"),
                    "set `default=\"Family Name\"` or `default=\"First, Fallback\"`, or omit it for a default-less declaration".to_string(),
                ))
            } else {
                Ok(VariableDefault::Font(names))
            }
        }
        VariableType::Image => {
            let abs = folder.join(text);
            if !abs.is_file() {
                Err((
                    "E_MISSING_FILE".to_string(),
                    format!(
                        "variable {who:?} default image path {text:?} does not exist or is not readable"
                    ),
                    "provide the file at the referenced path relative to the project folder, or fix the `default`".to_string(),
                ))
            } else if !path_readable(&abs) {
                Err((
                    "E_UNREADABLE".to_string(),
                    format!("variable {who:?} default image path {text:?} cannot be read"),
                    "make the file readable, or fix the `default`".to_string(),
                ))
            } else {
                Ok(VariableDefault::Image(text.to_string()))
            }
        }
    }
}

/// Validate one staged record into a typed declaration, exactly like
/// `check` validates a `variable` element: a required letter-led `id`,
/// a required type naming one of the seven declared types, an
/// optional `label`, (`enum` only) `options` with at least two
/// entries, and a `default` type-checked against the declaration.
/// Returns the typed declaration or the (`rule`, `message`, `fix`)
/// triple for the refusal.
pub(crate) fn record_to_decl(
    folder: &Path,
    raw: &VariableRecord,
) -> Result<VariableDecl, (String, String, String)> {
    if !valid_variable_id(&raw.id) {
        return Err((
            "E_BAD_ID".to_string(),
            format!(
                "variable `id` must start with a letter, then hold letters, digits, `-` or `_`; found {:?}",
                raw.id
            ),
            "set `id` to a name starting with a letter (for example `id=\"title\"`)".to_string(),
        ));
    }
    let Some(var_type) = VariableType::parse(&raw.var_type) else {
        return Err((
            "E_BAD_VARIABLE".to_string(),
            format!(
                "variable {:?} has unknown type {:?}; accepted types are: {}",
                raw.id,
                raw.var_type,
                VariableType::ACCEPTED.join(", ")
            ),
            "set `type` to one of `string`, `number`, `color`, `boolean`, `enum`, `font`, or `image`".to_string(),
        ));
    };
    let options = match var_type {
        VariableType::Enum => match &raw.options {
            Some(list) if list.len() >= 2 => Some(list.clone()),
            _ => {
                return Err((
                    "E_BAD_VARIABLE".to_string(),
                    format!(
                        "variable {:?} `enum` needs at least two `options`; found {}",
                        raw.id,
                        raw.options.as_ref().map(|l| l.len()).unwrap_or(0)
                    ),
                    "declare `options` with at least two comma-separated entries".to_string(),
                ));
            }
        },
        _ if raw.options.is_some() => {
            return Err((
                "E_BAD_VARIABLE".to_string(),
                format!(
                    "variable {:?} carries `options`, but only `enum` declarations hold `options`",
                    raw.id
                ),
                "remove `options`, or set `type=\"enum\"` with at least two entries".to_string(),
            ));
        }
        _ => None,
    };
    let default = match (&raw.default, var_type) {
        (None, _) => None,
        (Some(text), t) => Some(typed_default(folder, &raw.id, t, text, options.as_deref())?),
    };
    Ok(VariableDecl {
        id: raw.id.clone(),
        var_type,
        default,
        label: raw.label.clone(),
        options,
        line: 0,
        column: 0,
    })
}

/// Render one caller value for diagnostics, like `check` renders
/// `--variables` values in refusals.
fn override_text(value: &OverrideValue) -> String {
    match value {
        OverrideValue::Str(s) => format!("{s:?}"),
        OverrideValue::Num(x) => x.to_string(),
        OverrideValue::Bool(b) => b.to_string(),
    }
}

/// Coerce one caller override to its declaration's type, exactly like
/// `check` coerces `--variables` values: strings take any scalar,
/// numbers take finite decimals, colors take `#rgb`/`#rrggbb`,
/// booleans take `true`/`false`, enums take one `options` entry, fonts
/// take a non-empty comma try-order list (order preserved, never
/// resolved), images take a project-relative path that exists and
/// reads. Returns `Ok` or the (`rule`, `message`, `fix`) triple.
pub(crate) fn check_override_value(
    folder: &Path,
    decl_id: &str,
    var_type: VariableType,
    options: Option<&[String]>,
    value: &OverrideValue,
) -> Result<(), (String, String, String)> {
    const BAD: &str = "E_BAD_VARIABLE";
    let expected = match var_type {
        VariableType::String => String::new(),
        VariableType::Number => "a finite decimal number like `1.5`".to_string(),
        VariableType::Color => "a `#rgb` or `#rrggbb` color like `\"#ff3366\"`".to_string(),
        VariableType::Boolean => "`true` or `false`".to_string(),
        VariableType::Enum => String::new(),
        VariableType::Font => "a quoted font family list like `\"Family Name\"`".to_string(),
        VariableType::Image => {
            "a quoted project-relative image path like `\"media/photo.png\"`".to_string()
        }
    };
    let bad = |expected: String| -> (String, String, String) {
        (
            BAD.to_string(),
            format!(
                "`--variables` value for variable `--{decl_id}` of type `{}` holds {}; expected {expected}",
                var_type.name(),
                override_text(value),
            ),
            format!(
                "pass {expected} for `--{decl_id}` in the override delta, or remove the key to keep the declared default"
            ),
        )
    };
    match var_type {
        VariableType::String => Ok(()),
        VariableType::Number => match value {
            OverrideValue::Num(x) if x.is_finite() => Ok(()),
            OverrideValue::Str(s) => match s.trim().parse::<f64>() {
                Ok(x) if x.is_finite() => Ok(()),
                _ => Err(bad(expected)),
            },
            _ => Err(bad(expected)),
        },
        VariableType::Color => match value {
            OverrideValue::Str(s) => match notation::model::parse_color(s) {
                Some(_) => Ok(()),
                None => Err(bad(expected)),
            },
            _ => Err(bad(expected)),
        },
        VariableType::Boolean => match value {
            OverrideValue::Bool(_) => Ok(()),
            OverrideValue::Str(s) if s == "true" || s == "false" => Ok(()),
            _ => Err(bad(expected)),
        },
        VariableType::Enum => match value {
            OverrideValue::Str(s) => {
                let entries = options.unwrap_or(&[]);
                if entries.iter().any(|e| e == s) {
                    Ok(())
                } else {
                    let list = entries
                        .iter()
                        .map(|e| format!("{e:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    Err(bad(format!("one of its `options`: {list}")))
                }
            }
            _ => {
                let entries = options.unwrap_or(&[]);
                let list = entries
                    .iter()
                    .map(|e| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                Err(bad(format!("one of its `options`: {list}")))
            }
        },
        VariableType::Font => match value {
            OverrideValue::Str(s) => {
                if split_list(s).is_empty() {
                    Err(bad(expected))
                } else {
                    Ok(())
                }
            }
            _ => Err(bad(expected)),
        },
        VariableType::Image => match value {
            OverrideValue::Str(s) => {
                let abs = folder.join(s);
                if !abs.is_file() {
                    Err((
                        "E_MISSING_FILE".to_string(),
                        format!(
                            "`--variables` value for variable `--{decl_id}` image path {s:?} does not exist or is not readable"
                        ),
                        "provide the file at the referenced path relative to the project folder, or remove the key to keep the declared default".to_string(),
                    ))
                } else if !path_readable(&abs) {
                    Err((
                        "E_UNREADABLE".to_string(),
                        format!(
                            "`--variables` value for variable `--{decl_id}` image path {s:?} cannot be read"
                        ),
                        "make the file readable, or remove the key to keep the declared default".to_string(),
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Err(bad(expected)),
        },
    }
}

/// Read the persisted declaration delta: missing means no SDK-staged
/// declarations (empty, not an error). A corrupt file or another
/// version refuses the open naming the file, never guessed past.
pub(crate) fn read_variables_file(
    folder: &Path,
) -> Result<Vec<VariableRecord>, crate::OpenRefusal> {
    let path = sdk_variables_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(crate::OpenRefusal {
                kind: crate::RefusalKind::Invalid,
                message: format!(
                    "sdk refused: the SDK variable delta {} cannot be read ({e}); remove or fix the file and re-open. No composition was opened.",
                    path.display()
                ),
                diagnostics: Vec::new(),
            });
        }
    };
    let file: VariablesFile = serde_json::from_str(&raw).map_err(|e| crate::OpenRefusal {
        kind: crate::RefusalKind::Invalid,
        message: format!(
            "sdk refused: the SDK variable delta {} is corrupt ({e}); remove or fix the file and re-open. No composition was opened.",
            path.display()
        ),
        diagnostics: Vec::new(),
    })?;
    if file.version != SDK_VARIABLES_VERSION {
        return Err(crate::OpenRefusal {
            kind: crate::RefusalKind::Invalid,
            message: format!(
                "sdk refused: the SDK variable delta {} carries version {}, this SDK replays version {}; remove or migrate the file and re-open. No composition was opened.",
                path.display(),
                file.version,
                SDK_VARIABLES_VERSION
            ),
            diagnostics: Vec::new(),
        });
    }
    Ok(file.variables)
}

/// Read the persisted override delta: missing means no staged
/// variable overrides (empty, not an error). A corrupt file, another
/// version, or a non-scalar value refuses the open naming the file,
/// never guessed past.
pub(crate) fn read_variable_overrides_file(
    folder: &Path,
) -> Result<VariableOverrides, crate::OpenRefusal> {
    let path = sdk_variable_overrides_path_for(folder);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(VariableOverrides::new()),
        Err(e) => {
            return Err(crate::OpenRefusal {
                kind: crate::RefusalKind::Invalid,
                message: format!(
                    "sdk refused: the SDK variable override delta {} cannot be read ({e}); remove or fix the file and re-open. No composition was opened.",
                    path.display()
                ),
                diagnostics: Vec::new(),
            });
        }
    };
    let file: VariableOverridesFile =
        serde_json::from_str(&raw).map_err(|e| crate::OpenRefusal {
            kind: crate::RefusalKind::Invalid,
            message: format!(
                "sdk refused: the SDK variable override delta {} is corrupt ({e}); remove or fix the file and re-open. No composition was opened.",
                path.display()
            ),
            diagnostics: Vec::new(),
        })?;
    if file.version != SDK_VARIABLES_VERSION {
        return Err(crate::OpenRefusal {
            kind: crate::RefusalKind::Invalid,
            message: format!(
                "sdk refused: the SDK variable override delta {} carries version {}, this SDK replays version {}; remove or migrate the file and re-open. No composition was opened.",
                path.display(),
                file.version,
                SDK_VARIABLES_VERSION
            ),
            diagnostics: Vec::new(),
        });
    }
    let mut out = VariableOverrides::new();
    for (key, val) in file.overrides {
        let parsed = match val {
            serde_json::Value::String(s) => OverrideValue::Str(s),
            serde_json::Value::Number(n) => match n.as_f64() {
                Some(x) if x.is_finite() => OverrideValue::Num(x),
                _ => {
                    return Err(crate::OpenRefusal {
                        kind: crate::RefusalKind::Invalid,
                        message: format!(
                            "sdk refused: the SDK variable override delta {} holds a non-scalar value for `--{key}`; remove or fix the file and re-open. No composition was opened.",
                            path.display()
                        ),
                        diagnostics: Vec::new(),
                    });
                }
            },
            serde_json::Value::Bool(b) => OverrideValue::Bool(b),
            _ => {
                return Err(crate::OpenRefusal {
                    kind: crate::RefusalKind::Invalid,
                    message: format!(
                        "sdk refused: the SDK variable override delta {} holds a non-scalar value for `--{key}`; remove or fix the file and re-open. No composition was opened.",
                        path.display()
                    ),
                    diagnostics: Vec::new(),
                });
            }
        };
        out.insert(key, parsed);
    }
    Ok(out)
}

/// Write the declaration delta as a whole-log rewrite (every staged
/// record, so replays stay ordered and idempotent).
pub(crate) fn write_variables_file(
    folder: &Path,
    records: &[VariableRecord],
) -> Result<(), String> {
    let path = sdk_variables_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "the hidden SDK directory {} cannot be prepared: {e}",
                parent.display()
            )
        })?;
    }
    let file = VariablesFile {
        version: SDK_VARIABLES_VERSION,
        variables: records.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("the variable delta cannot be encoded: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| {
        format!(
            "the SDK variable delta {} cannot be written: {e}",
            path.display()
        )
    })
}

/// Write the override delta as a whole-map rewrite (every persisted
/// plus staged override, so replays stay idempotent).
pub(crate) fn write_variable_overrides_file(
    folder: &Path,
    overrides: &VariableOverrides,
) -> Result<(), String> {
    let path = sdk_variable_overrides_path_for(folder);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "the hidden SDK directory {} cannot be prepared: {e}",
                parent.display()
            )
        })?;
    }
    let mut scalars = BTreeMap::new();
    for (key, value) in overrides {
        let scalar = match value {
            OverrideValue::Str(s) => serde_json::Value::String(s.clone()),
            OverrideValue::Num(x) => serde_json::Value::Number(
                serde_json::Number::from_f64(*x)
                    .ok_or_else(|| format!("override `--{key}` holds a non-finite number"))?,
            ),
            OverrideValue::Bool(b) => serde_json::Value::Bool(*b),
        };
        scalars.insert(key.clone(), scalar);
    }
    let file = VariableOverridesFile {
        version: SDK_VARIABLES_VERSION,
        overrides: scalars,
    };
    let raw = serde_json::to_string_pretty(&file)
        .map_err(|e| format!("the variable override delta cannot be encoded: {e}"))?;
    std::fs::write(&path, raw).map_err(|e| {
        format!(
            "the SDK variable override delta {} cannot be written: {e}",
            path.display()
        )
    })
}

/// Collect every `*.kdl` source under `folder` in sorted relative
/// order, skipping hidden directories (the SDK's own deltas live
/// there and carry no `var()` calls).
fn kdl_sources(folder: &Path) -> Vec<(String, String)> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .map(|r| r.flatten().collect())
            .unwrap_or_default();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with('.'))
                    .unwrap_or(true)
                {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) == Some("kdl") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut out = Vec::new();
    for path in files {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let rel = path
                .strip_prefix(folder)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            out.push((rel, text));
        }
    }
    out
}

/// Advance one Unicode scalar from byte offset `j`.
fn adv_char(raw: &str, j: usize) -> usize {
    j + raw[j..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)
}

/// Skip ASCII whitespace from byte offset `j`.
fn skip_ws(raw: &str, mut j: usize) -> usize {
    while j < raw.len() {
        match raw.as_bytes()[j] {
            b' ' | b'\t' | b'\n' | b'\r' => j += 1,
            _ => break,
        }
    }
    j
}

/// Whether `var(` at byte offset `abs` really opens a call: the
/// preceding byte is no name character, so `myvar(` never counts.
fn is_call_start(raw: &str, abs: usize) -> bool {
    if abs == 0 {
        return true;
    }
    !matches!(
        raw.as_bytes()[abs - 1],
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.'
    )
}

/// Parse the single `var()` call opening at byte offset `abs` (the
/// `v` of `var(`): the referenced id, or `None` when the text is no
/// well-formed call (malformed calls belong to validation, never to
/// the scan). The scan resumes past the id, so fallbacks holding
/// nested `var()` calls surface on their own.
fn parse_var_call(raw: &str, abs: usize) -> Option<(String, usize)> {
    let mut j = skip_ws(raw, abs + 4);
    if !raw[j..].starts_with("--") {
        return None;
    }
    j += 2;
    let start = j;
    while j < raw.len() {
        match raw.as_bytes()[j] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' => j += 1,
            _ => break,
        }
    }
    let name = raw.get(start..j)?.to_string();
    if !valid_variable_id(&name) {
        return None;
    }
    j = skip_ws(raw, j);
    if j >= raw.len() {
        return None;
    }
    match raw.as_bytes()[j] {
        b')' => Some((name, j + 1)),
        b',' => Some((name, j + 1)),
        _ => None,
    }
}

/// Nearest `key=` before byte offset `abs` on the same line: the
/// hyphenated attribute holding the call, as authored. Empty when the
/// call sits where no attribute key precedes it.
fn attribute_at(raw: &str, abs: usize) -> String {
    let line_start = raw[..abs].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let head = &raw[line_start..abs];
    let bytes = head.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        if bytes[i - 1] == b'=' {
            let mut end = i - 1;
            while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
                end -= 1;
            }
            let mut start = end;
            while start > 0
                && matches!(
                    bytes[start - 1],
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'
                )
            {
                start -= 1;
            }
            if start < end {
                if let Some(first) = head[start..].chars().next() {
                    if first.is_ascii_alphabetic() || first == '_' {
                        return head[start..end].to_string();
                    }
                }
            }
            i = start;
        } else {
            i -= 1;
        }
    }
    String::new()
}

/// Scan one source for every `var(--id)` reference: the variable id
/// with the 1-based line and character column of the call plus the
/// hyphenated attribute holding it. Malformed calls are skipped
/// (validation owns syntax errors); nested fallbacks surface each
/// inner call on its own.
fn scan_text(file: &str, raw: &str) -> Vec<VariableUse> {
    let mut line_starts = vec![0usize];
    for (i, b) in raw.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    let locate = |abs: usize| -> (usize, usize) {
        let line = match line_starts.binary_search(&abs) {
            Ok(l) => l,
            Err(0) => 0,
            Err(l) => l - 1,
        };
        let col = raw[line_starts[line]..abs].chars().count() + 1;
        (line + 1, col)
    };
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 <= raw.len() {
        match raw[i..].find("var(") {
            None => break,
            Some(rel) => {
                let abs = i + rel;
                if !is_call_start(raw, abs) {
                    i = abs + 4;
                    continue;
                }
                match parse_var_call(raw, abs) {
                    None => {
                        i = adv_char(raw, abs);
                    }
                    Some((id, next)) => {
                        let (line, column) = locate(abs);
                        out.push(VariableUse {
                            variable: id,
                            file: file.to_string(),
                            line,
                            column,
                            attribute: attribute_at(raw, abs),
                        });
                        i = next.max(abs + 1);
                    }
                }
            }
        }
    }
    out
}

/// Scan one project folder for every `var(--id)` reference across its
/// KDL sources, in file, line, and column order. Pure text: staged
/// (not yet persisted) declarations add no references, and malformed
/// calls stay with validation.
pub fn scan_folder(folder: &Path) -> Vec<VariableUse> {
    let mut out = Vec::new();
    for (file, text) in kdl_sources(folder) {
        out.extend(scan_text(&file, &text));
    }
    out
}

/// Location file for offenders on staged declarations: they live in
/// memory, not on disk, so no source line owns them.
pub const STAGED_DECLARATIONS_FILE: &str = "<sdk-staged>";

/// Location file for offenders on staged override values.
pub const STAGED_OVERRIDES_FILE: &str = "<sdk-staged-overrides>";

fn read_only_refusal() -> VariableRefusal {
    refused(
        VariableRefusalKind::ReadOnly,
        crate::edits::E_SDK_READ_ONLY,
        "sdk refused: the session opened read-only (`persist: false`); variables never stage. Re-open with `persist: true` to declare or override. Nothing was staged, applied, or recorded.".to_string(),
    )
}

impl crate::SdkSession {
    /// Declared-variable lookup for one id: on-disk declarations
    /// across every composition plus staged records, first wins
    /// (declare and replay refuse duplicates, so winners never
    /// conflict).
    fn declaration_table(&self) -> BTreeMap<String, (VariableType, Option<Vec<String>>)> {
        let mut table = BTreeMap::new();
        for comp in self
            .all_compositions()
            .iter()
            .chain(std::iter::once(self.effective_composition()))
        {
            for decl in &comp.variables {
                table
                    .entry(decl.id.clone())
                    .or_insert((decl.var_type, decl.options.clone()));
            }
        }
        table
    }

    /// Ids declared on disk (every composition), without staged
    /// records: the duplicate baseline for staged declarations.
    fn disk_variable_ids(&self) -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        for comp in self.all_compositions() {
            for decl in &comp.variables {
                ids.insert(decl.id.clone());
            }
        }
        ids
    }

    /// Declare one variable on the open composition (Sprint 15, T004):
    /// rule checks run first (id syntax, known type, options shape,
    /// default type-checked with font try-order preserved and image
    /// presence verified), then the declaration stages in memory and
    /// shows in [`SdkSession::effective_composition`](crate::SdkSession::effective_composition).
    /// Nothing is written to disk; [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
    /// saves staged declarations later. A refused declaration names
    /// [`E_SDK_BAD_VARIABLE`] with the rule and the fix, staging
    /// nothing. Read-only sessions refuse with `E_SDK_READ_ONLY`;
    /// preview sessions stage normally and refuse only at persist.
    pub fn declare_variable(
        &mut self,
        input: VariableInput,
    ) -> Result<DeclaredVariable, VariableRefusal> {
        if !self.persist {
            return Err(read_only_refusal());
        }
        let options_list = input
            .options
            .as_deref()
            .map(split_list)
            .filter(|l| !l.is_empty());
        let record = VariableRecord {
            id: input.id.clone(),
            var_type: input.var_type.clone(),
            default: input.default.clone(),
            label: input.label.clone(),
            options: options_list,
        };
        let mut taken = self.disk_variable_ids();
        for staged in &self.variable_decls {
            taken.insert(staged.raw.id.clone());
        }
        if taken.contains(&record.id) {
            return Err(refused(
                VariableRefusalKind::BadVariable,
                E_SDK_BAD_VARIABLE,
                format!(
                    "sdk refused: variable `--{}` is already declared in the project; `id` values stay unique across the project. Nothing was staged, applied, or recorded.",
                    record.id
                ),
            ));
        }
        let decl = record_to_decl(&self.folder, &record).map_err(|(rule, message, fix)| {
            refused(
                VariableRefusalKind::BadVariable,
                E_SDK_BAD_VARIABLE,
                format!("sdk refused: {message} (rule {rule}; fix: {fix}). Nothing was staged, applied, or recorded."),
            )
        })?;
        let read_back = DeclaredVariable {
            id: decl.id.clone(),
            var_type: decl.var_type.name().to_string(),
            default: decl.default.as_ref().map(canonical_default_text),
            label: decl.label.clone(),
        };
        self.variable_decls.push(StagedVariable {
            raw: record,
            persisted: false,
        });
        self.composition.variables.push(decl);
        Ok(read_back)
    }

    /// Edit one SDK-staged declaration (Sprint 18, variables panel):
    /// the same rule checks as [`SdkSession::declare_variable`] run
    /// first (id syntax, known type, options shape, default
    /// type-checked with font try-order preserved and image presence
    /// verified), then the staged record rewrites in memory and the
    /// open composition entry follows. Nothing is written to disk;
    /// [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
    /// saves the rewrite later. A refused edit names
    /// [`E_SDK_BAD_VARIABLE`] with the rule and the fix, changing
    /// nothing. An id with no staged record refuses: an on-disk
    /// declaration lives in its KDL source (edit the `variable`
    /// element there), an unknown id names
    /// [`E_SDK_UNKNOWN_VARIABLE`]. Renaming (an input id differing
    /// from `id`) keeps uniqueness across the project. Read-only
    /// sessions refuse with `E_SDK_READ_ONLY`; preview sessions edit
    /// normally and refuse only at persist.
    pub fn edit_variable(
        &mut self,
        id: &str,
        input: VariableInput,
    ) -> Result<DeclaredVariable, VariableRefusal> {
        if !self.persist {
            return Err(read_only_refusal());
        }
        let pos = self.variable_decls.iter().position(|s| s.raw.id == id);
        let Some(pos) = pos else {
            if self.disk_variable_ids().contains(id) {
                return Err(refused(
                    VariableRefusalKind::BadVariable,
                    E_SDK_BAD_VARIABLE,
                    format!(
                        "sdk refused: variable `--{id}` lives in the project sources, not in the staged set; edit the `variable id=\"{id}\"` element in its KDL source. Nothing was staged, applied, or recorded."
                    ),
                ));
            }
            return Err(refused(
                VariableRefusalKind::UnknownVariable,
                E_SDK_UNKNOWN_VARIABLE,
                format!(
                    "sdk refused: the staged set declares no variable `--{id}`; no `variable` in the project declares it either. Declare it with `declare_variable` first. Nothing was staged, applied, or recorded."
                ),
            ));
        };
        let options_list = input
            .options
            .as_deref()
            .map(split_list)
            .filter(|l| !l.is_empty());
        let record = VariableRecord {
            id: input.id.clone(),
            var_type: input.var_type.clone(),
            default: input.default.clone(),
            label: input.label.clone(),
            options: options_list,
        };
        if record.id != id {
            let mut taken = self.disk_variable_ids();
            for (i, staged) in self.variable_decls.iter().enumerate() {
                if i != pos {
                    taken.insert(staged.raw.id.clone());
                }
            }
            if taken.contains(&record.id) {
                return Err(refused(
                    VariableRefusalKind::BadVariable,
                    E_SDK_BAD_VARIABLE,
                    format!(
                        "sdk refused: variable `--{}` is already declared in the project; `id` values stay unique across the project. Nothing was staged, applied, or recorded.",
                        record.id
                    ),
                ));
            }
        }
        let decl = record_to_decl(&self.folder, &record).map_err(|(rule, message, fix)| {
            refused(
                VariableRefusalKind::BadVariable,
                E_SDK_BAD_VARIABLE,
                format!("sdk refused: {message} (rule {rule}; fix: {fix}). Nothing was staged, applied, or recorded."),
            )
        })?;
        let read_back = DeclaredVariable {
            id: decl.id.clone(),
            var_type: decl.var_type.name().to_string(),
            default: decl.default.as_ref().map(canonical_default_text),
            label: decl.label.clone(),
        };
        self.variable_decls[pos] = StagedVariable {
            raw: record,
            persisted: false,
        };
        if let Some(slot) = self.composition.variables.iter_mut().find(|d| d.id == id) {
            *slot = decl;
        } else {
            self.composition.variables.push(decl);
        }
        Ok(read_back)
    }

    /// Remove one SDK-staged declaration (Sprint 18, variables
    /// panel): the staged record drops in memory and the open
    /// composition entry follows; [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
    /// saves the removal later. A variable still referenced by
    /// `var(--id)` in the project sources refuses with
    /// [`E_SDK_BAD_VARIABLE`] listing every reference, removing
    /// nothing. An id with no staged record refuses like
    /// [`SdkSession::edit_variable`]: on-disk declarations live in
    /// their KDL source, unknown ids name
    /// [`E_SDK_UNKNOWN_VARIABLE`]. Read-only sessions refuse with
    /// `E_SDK_READ_ONLY`; preview sessions remove normally and
    /// refuse only at persist.
    pub fn remove_variable(&mut self, id: &str) -> Result<VariableRecord, VariableRefusal> {
        if !self.persist {
            return Err(read_only_refusal());
        }
        let pos = self.variable_decls.iter().position(|s| s.raw.id == id);
        let Some(pos) = pos else {
            if self.disk_variable_ids().contains(id) {
                return Err(refused(
                    VariableRefusalKind::BadVariable,
                    E_SDK_BAD_VARIABLE,
                    format!(
                        "sdk refused: variable `--{id}` lives in the project sources, not in the staged set; remove the `variable id=\"{id}\"` element in its KDL source. Nothing was staged, applied, or recorded."
                    ),
                ));
            }
            return Err(refused(
                VariableRefusalKind::UnknownVariable,
                E_SDK_UNKNOWN_VARIABLE,
                format!(
                    "sdk refused: the staged set declares no variable `--{id}`; no `variable` in the project declares it either, so there is nothing to remove. Nothing was staged, applied, or recorded."
                ),
            ));
        };
        let mut uses: Vec<VariableUse> = scan_folder(&self.folder)
            .into_iter()
            .filter(|u| u.variable == id)
            .collect();
        uses.sort_by(|a, b| {
            (&a.file, a.line, a.column, &a.attribute).cmp(&(&b.file, b.line, b.column, &b.attribute))
        });
        if !uses.is_empty() {
            let list = uses
                .iter()
                .map(|u| format!("{}:{}:{} (`{}`)", u.file, u.line, u.column, u.attribute))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(refused(
                VariableRefusalKind::BadVariable,
                E_SDK_BAD_VARIABLE,
                format!(
                    "sdk refused: variable `--{id}` is still in use by {}; unbind every reference first, then remove it. Nothing was staged, applied, or recorded.",
                    list
                ),
            ));
        }
        let staged = self.variable_decls.remove(pos);
        self.composition.variables.retain(|d| d.id != id);
        self.variables_pruned = true;
        Ok(staged.raw)
    }

    /// Every variable declaration visible in this session: on-disk
    /// declarations plus staged records on the open composition, in
    /// that order. Override deltas never mutate these defaults.
    pub fn variable_declarations(&self) -> &[VariableDecl] {
        &self.composition.variables
    }

    /// Staged declaration records not yet in the delta file, in
    /// declare order (replayed and previously persisted records
    /// excluded).
    pub fn staged_variable_records(&self) -> Vec<VariableRecord> {
        self.variable_decls
            .iter()
            .filter(|s| !s.persisted)
            .map(|s| s.raw.clone())
            .collect()
    }

    /// Validate the whole variable set (Sprint 15, T004): a strict
    /// project pass (every disk offender with its check rule and
    /// location) plus the staged set (declarations re-validated,
    /// staged-but-never-bound and default-less-without-value reported,
    /// staged override values re-coerced). Empty means valid. Sorted
    /// by file, line, column, rule, and message so repeated runs
    /// agree.
    pub fn validate_variables(&self) -> Vec<VariableOffender> {
        let (mut hard, soft) = self.variable_offenders();
        hard.extend(soft);
        hard.sort_by(|a, b| {
            (&a.file, a.line, a.column, &a.rule, &a.message)
                .cmp(&(&b.file, b.line, b.column, &b.rule, &b.message))
        });
        hard
    }

    /// Blocking offenders only (Sprint 18, variables panel): failed
    /// rules, duplicates, unknown or mistyped staged overrides, and
    /// disk diagnostics — everything
    /// [`SdkSession::persist_variables_with_warnings`](crate::SdkSession::persist_variables_with_warnings)
    /// refuses on. Staged-but-never-bound and
    /// default-less-without-value advisories are excluded: those
    /// persist as warnings. Sorted by file, line, column, rule, and
    /// message so repeated runs agree.
    pub fn variable_hard_offenders(&self) -> Vec<VariableOffender> {
        let (mut hard, _) = self.variable_offenders();
        hard.sort_by(|a, b| {
            (&a.file, a.line, a.column, &a.rule, &a.message)
                .cmp(&(&b.file, b.line, b.column, &b.rule, &b.message))
        });
        hard
    }

    /// Persist-blocking offenders only (Sprint 18, variables
    /// panel): [`SdkSession::variable_hard_offenders`](crate::SdkSession::variable_hard_offenders)
    /// minus tolerated disk health (strict unused and default-less
    /// diagnostics, which persist as listed warnings instead of
    /// deadlocking panel flows). Sorted like the hard list.
    pub fn variable_blocking_offenders(&self) -> Vec<VariableOffender> {
        let (mut blocking, _) = partition_hard_offenders(self.variable_hard_offenders());
        blocking.sort_by(|a, b| {
            (&a.file, a.line, a.column, &a.rule, &a.message)
                .cmp(&(&b.file, b.line, b.column, &b.rule, &b.message))
        });
        blocking
    }

    /// The same offender set as
    /// [`SdkSession::validate_variables`](crate::SdkSession::validate_variables),
    /// split into blocking (`hard`) and advisory (`soft`) offenders.
    /// Hard offenders are failed rules: disk diagnostics, duplicate
    /// or invalid staged declarations, and unknown or mistyped
    /// staged override values — persistence is blocked while any
    /// stands. Soft offenders are staged declarations that validate
    /// yet have no effect yet: never-bound and default-less-without-
    /// value. [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
    /// blocks on both; panel persistence
    /// ([`SdkSession::persist_variables_with_warnings`](crate::SdkSession::persist_variables_with_warnings))
    /// blocks on hard offenders and returns soft ones as warnings to
    /// list. Each list is unsorted; callers sort for display.
    fn variable_offenders(&self) -> (Vec<VariableOffender>, Vec<VariableOffender>) {
        let mut out: Vec<VariableOffender> = Vec::new();
        let mut soft: Vec<VariableOffender> = Vec::new();
        let merged = self.variable_overrides();
        let verdict = validate::validate::validate_folder_with_variables_strict_themed(
            &self.folder,
            &merged,
            true,
            None,
        );
        // Staged declarations live in memory, not on disk, so the
        // disk pass reports staged override keys as unknown: those
        // diagnostics belong to the staged set below, never here.
        let staged_ids: BTreeSet<String> = self
            .variable_decls
            .iter()
            .map(|s| s.raw.id.clone())
            .collect();
        for d in &verdict.diagnostics {
            if d.message.contains("variable")
                && !staged_ids.iter().any(|id| {
                    d.message
                        .contains(&format!("unknown override key `--{id}`"))
                })
            {
                out.push(VariableOffender {
                    rule: d.code.to_string(),
                    file: d.file.clone(),
                    line: d.line,
                    column: d.column,
                    message: d.message.clone(),
                    fix: d.fix.clone(),
                });
            }
        }
        let disk_ids = self.disk_variable_ids();
        let bound: BTreeSet<String> = scan_folder(&self.folder)
            .iter()
            .map(|u| u.variable.clone())
            .collect();
        let table = self.declaration_table();
        for staged in &self.variable_decls {
            if disk_ids.contains(&staged.raw.id) {
                out.push(VariableOffender {
                    rule: E_SDK_BAD_VARIABLE.to_string(),
                    file: STAGED_DECLARATIONS_FILE.to_string(),
                    line: 0,
                    column: 0,
                    message: format!(
                        "staged variable `--{}` duplicates an on-disk declaration; `id` values stay unique across the project",
                        staged.raw.id
                    ),
                    fix: format!(
                        "remove the staged `--{}` declaration, or remove the on-disk `variable id=\"{}\"`",
                        staged.raw.id, staged.raw.id
                    ),
                });
                continue;
            }
            match record_to_decl(&self.folder, &staged.raw) {
                Err((rule, message, fix)) => out.push(VariableOffender {
                    rule,
                    file: STAGED_DECLARATIONS_FILE.to_string(),
                    line: 0,
                    column: 0,
                    message,
                    fix,
                }),
                Ok(decl) => {
                    if !bound.contains(&decl.id) && !merged.contains_key(&decl.id) {
                        soft.push(VariableOffender {
                            rule: "E_BAD_VARIABLE".to_string(),
                            file: STAGED_DECLARATIONS_FILE.to_string(),
                            line: 0,
                            column: 0,
                            message: format!(
                                "staged variable `--{0}` is declared but never bound by `var(--{0})`; the declaration has no effect",
                                decl.id
                            ),
                            fix: format!(
                                "bind it with `var(--{0})` in text, media, or an ordinary attribute, or drop the staged declaration",
                                decl.id
                            ),
                        });
                    }
                    if decl.default.is_none() && !merged.contains_key(&decl.id) {
                        soft.push(VariableOffender {
                            rule: "E_BAD_VARIABLE".to_string(),
                            file: STAGED_DECLARATIONS_FILE.to_string(),
                            line: 0,
                            column: 0,
                            message: format!(
                                "staged default-less variable `--{}` has no value; the declaration carries no `default` and no override value was staged",
                                decl.id
                            ),
                            fix: format!(
                                "stage an override value for `--{0}`, or re-declare `--{0}` with `default=<value>` matching its type",
                                decl.id
                            ),
                        });
                    }
                }
            }
        }
        for (key, value) in &self.staged_var_overrides {
            match table.get(key) {
                None => out.push(VariableOffender {
                    rule: "E_BAD_VARIABLE".to_string(),
                    file: STAGED_OVERRIDES_FILE.to_string(),
                    line: 0,
                    column: 0,
                    message: format!(
                        "staged override names undeclared variable `--{key}`; no `variable` in the project declares `--{key}`, so the value matches nothing"
                    ),
                    fix: format!(
                        "remove the key `--{key}` from the staged overrides, or declare `variable id=\"{key}\" type=\"...\"`"
                    ),
                }),
                Some((var_type, options)) => {
                    if let Err((rule, message, fix)) =
                        check_override_value(&self.folder, key, *var_type, options.as_deref(), value)
                    {
                        out.push(VariableOffender {
                            rule,
                            file: STAGED_OVERRIDES_FILE.to_string(),
                            line: 0,
                            column: 0,
                            message,
                            fix,
                        });
                    }
                }
            }
        }
        (out, soft)
    }

    /// Scan every `var(--id)` reference across the project's KDL
    /// sources, in file, line, and column order. With a filter only
    /// references to that variable list; without one every reference
    /// lists.
    pub fn scan_variable_usage(&self, filter: Option<&str>) -> Vec<VariableUse> {
        scan_folder(&self.folder)
            .into_iter()
            .filter(|u| filter.map(|id| u.variable == id).unwrap_or(true))
            .collect()
    }

    /// Merged override values for this session: open-time overrides,
    /// then replayed and previously persisted SDK overrides, then
    /// staged values (later wins). Declared defaults stay untouched:
    /// overrides resolve at bind time, never by rewriting
    /// declarations.
    pub fn variable_overrides(&self) -> VariableOverrides {
        let mut merged = self.applied_overrides.clone();
        for (key, value) in &self.persisted_var_overrides {
            merged.insert(key.clone(), value.clone());
        }
        for (key, value) in &self.staged_var_overrides {
            merged.insert(key.clone(), value.clone());
        }
        merged
    }

    /// Staged override values not yet in the delta file.
    pub fn staged_variable_overrides(&self) -> VariableOverrides {
        self.staged_var_overrides.clone()
    }

    /// Apply one override delta (Sprint 15, T004): every key must name
    /// a declared variable and every value must fit its type (font
    /// lists keep comma try-order, images must exist and read). The
    /// delta stages in memory without touching declared defaults and
    /// shows in [`SdkSession::variable_overrides`](crate::SdkSession::variable_overrides);
    /// [`SdkSession::persist_variables`](crate::SdkSession::persist_variables)
    /// saves it later. Undeclared keys refuse the whole delta with
    /// [`E_SDK_UNKNOWN_VARIABLE`] listing each one — never silently
    /// created, nothing staged. Mistyped values refuse with
    /// [`E_SDK_BAD_VARIABLE`], staging nothing. Read-only sessions
    /// refuse with `E_SDK_READ_ONLY`; preview sessions stage normally
    /// and refuse only at persist.
    pub fn apply_variable_overrides(
        &mut self,
        delta: VariableOverrides,
    ) -> Result<OverrideDeltaReport, VariableRefusal> {
        if !self.persist {
            return Err(read_only_refusal());
        }
        let table = self.declaration_table();
        let mut undeclared: Vec<String> = delta
            .keys()
            .filter(|k| !table.contains_key(*k))
            .cloned()
            .collect();
        undeclared.sort();
        if !undeclared.is_empty() {
            let list = undeclared
                .iter()
                .map(|k| format!("`--{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(refused(
                VariableRefusalKind::UnknownVariable,
                E_SDK_UNKNOWN_VARIABLE,
                format!(
                    "sdk refused: the override delta names undeclared variable(s) {list}; no `variable` in the project declares them, so the values match nothing. Declare each with `declare_variable` or remove the keys. Nothing was staged, applied, or recorded."
                ),
            ));
        }
        let mut mistyped: Vec<String> = Vec::new();
        for (key, value) in &delta {
            let (var_type, options) = table.get(key).expect("declared key has a type");
            if let Err((rule, message, _)) =
                check_override_value(&self.folder, key, *var_type, options.as_deref(), value)
            {
                mistyped.push(format!("{message} (rule {rule})"));
            }
        }
        if !mistyped.is_empty() {
            mistyped.sort();
            return Err(refused(
                VariableRefusalKind::BadVariable,
                E_SDK_BAD_VARIABLE,
                format!(
                    "sdk refused: the override delta holds {} mistyped value(s): {}. Nothing was staged, applied, or recorded.",
                    mistyped.len(),
                    mistyped.join("; ")
                ),
            ));
        }
        let mut applied: Vec<String> = delta.keys().cloned().collect();
        applied.sort();
        for (key, value) in delta {
            self.staged_var_overrides.insert(key, value);
        }
        Ok(OverrideDeltaReport { applied })
    }

    /// Persist staged declarations and override deltas (Sprint 15,
    /// T004): validation runs first and a non-empty offender list
    /// refuses with [`E_SDK_INVALID_VARIABLES`] writing nothing — no
    /// partial save. Otherwise the declaration delta
    /// (`<project>/.framesmith/sdk-variables.json`) and the override
    /// delta (`<project>/.framesmith/sdk-variable-overrides.json`)
    /// write as whole-log rewrites; composition sources stay
    /// byte-identical and no render output is produced. Persisting a
    /// clean stage writes nothing and reports zero. Preview sessions
    /// refuse with `E_SDK_PREVIEW`, read-only sessions with
    /// `E_SDK_READ_ONLY`, an unwritable hidden directory with
    /// `E_SDK_UNWRITABLE`.
    pub fn persist_variables(&mut self) -> Result<VariablePersistReport, VariableRefusal> {
        self.check_persist_capability()?;
        let offenders = self.validate_variables();
        if !offenders.is_empty() {
            return Err(invalid_variables_refusal(&offenders));
        }
        self.write_staged_variables()
    }

    /// Persist staged declarations and override deltas, returning
    /// advisory warnings (Sprint 18, variables panel): blocking
    /// offenders (failed rules, duplicates, unknown or mistyped
    /// staged overrides, non-health disk diagnostics) refuse with
    /// [`E_SDK_INVALID_VARIABLES`] writing nothing — exactly like
    /// [`SdkSession::persist_variables`](crate::SdkSession::persist_variables).
    /// Soft offenders (staged-but-never-bound and
    /// default-less-without-value declarations) plus tolerated
    /// disk health (strict unused and default-less diagnostics)
    /// persist anyway and return as warnings for the caller to
    /// list on the panel: an unused declaration is a warning,
    /// never a second store and never a silent drop. Capability
    /// refusals (`E_SDK_PREVIEW`, `E_SDK_READ_ONLY`,
    /// `E_SDK_UNWRITABLE`) match the strict path.
    pub fn persist_variables_with_warnings(
        &mut self,
    ) -> Result<VariablePersistOutcome, VariableRefusal> {
        self.check_persist_capability()?;
        let (hard, mut soft) = self.variable_offenders();
        let (blocking, mut advisory) = partition_hard_offenders(hard);
        if !blocking.is_empty() {
            return Err(invalid_variables_refusal(&blocking));
        }
        soft.append(&mut advisory);
        soft.sort_by(|a, b| {
            (&a.file, a.line, a.column, &a.rule, &a.message)
                .cmp(&(&b.file, b.line, b.column, &b.rule, &b.message))
        });
        let report = self.write_staged_variables()?;
        Ok(VariablePersistOutcome {
            report,
            warnings: soft,
        })
    }

    /// Capability gate shared by both persist paths: preview and
    /// read-only sessions refuse before validation runs.
    fn check_persist_capability(&self) -> Result<(), VariableRefusal> {
        if self.preview {
            return Err(refused(
                VariableRefusalKind::Preview,
                crate::edits::E_SDK_PREVIEW,
                "sdk refused: the session opened as a dry-run preview; previews never persist. Re-open without `preview` to persist. Nothing was staged, applied, or recorded.".to_string(),
            ));
        }
        if !self.persist {
            return Err(read_only_refusal());
        }
        Ok(())
    }

    /// Whole-log rewrite of the staged declaration and override
    /// deltas, shared by both persist paths. Composition sources
    /// stay byte-identical and no render output is produced.
    /// A removal since the last persist rewrites the declaration
    /// delta even with nothing newly staged, so removals land.
    fn write_staged_variables(&mut self) -> Result<VariablePersistReport, VariableRefusal> {
        let staged_records: Vec<VariableRecord> = self
            .variable_decls
            .iter()
            .filter(|s| !s.persisted)
            .map(|s| s.raw.clone())
            .collect();
        let staged_overrides = self.staged_var_overrides.len();
        let pruned = self.variables_pruned;
        if staged_records.is_empty() && staged_overrides == 0 && !pruned {
            return Ok(VariablePersistReport {
                declarations: 0,
                overrides: 0,
                paths: Vec::new(),
            });
        }
        let mut paths = Vec::new();
        if !staged_records.is_empty() || pruned {
            let all: Vec<VariableRecord> =
                self.variable_decls.iter().map(|s| s.raw.clone()).collect();
            write_variables_file(&self.folder, &all).map_err(|why| {
                refused(
                    VariableRefusalKind::Unwritable,
                    crate::edits::E_SDK_UNWRITABLE,
                    format!("sdk refused: {why}; history is unavailable. Nothing was persisted."),
                )
            })?;
            paths.push(sdk_variables_path_for(&self.folder));
        }
        if staged_overrides > 0 {
            let mut merged = self.persisted_var_overrides.clone();
            for (key, value) in &self.staged_var_overrides {
                merged.insert(key.clone(), value.clone());
            }
            write_variable_overrides_file(&self.folder, &merged).map_err(|why| {
                refused(
                    VariableRefusalKind::Unwritable,
                    crate::edits::E_SDK_UNWRITABLE,
                    format!("sdk refused: {why}; history is unavailable. Nothing was persisted."),
                )
            })?;
            paths.push(sdk_variable_overrides_path_for(&self.folder));
        }
        for staged in &mut self.variable_decls {
            staged.persisted = true;
        }
        self.variables_pruned = false;
        let staged_map = std::mem::take(&mut self.staged_var_overrides);
        for (key, value) in staged_map {
            self.persisted_var_overrides.insert(key, value);
        }
        Ok(VariablePersistReport {
            declarations: staged_records.len(),
            overrides: staged_overrides,
            paths,
        })
    }

    /// SDK variable declaration delta path replayed at open and
    /// written by [`SdkSession::persist_variables`](crate::SdkSession::persist_variables):
    /// `<project>/.framesmith/sdk-variables.json`.
    pub fn variable_declarations_path(&self) -> PathBuf {
        sdk_variables_path_for(&self.folder)
    }

    /// SDK variable override delta path written by
    /// [`SdkSession::persist_variables`](crate::SdkSession::persist_variables):
    /// `<project>/.framesmith/sdk-variable-overrides.json`.
    pub fn variable_overrides_path(&self) -> PathBuf {
        sdk_variable_overrides_path_for(&self.folder)
    }
}
