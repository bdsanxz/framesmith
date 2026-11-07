//! Variable declare, validate, scan, and override deltas (Sprint 15,
//! T004 / CAP-005): declarations accept font and image value objects,
//! validation reports every offender with rule and location, the usage
//! scan lists every reference, override deltas apply without touching
//! declared defaults, undeclared overrides refuse without creating,
//! and failed validation blocks persistence with nothing saved.

use sdk::{
    OpenOptions, SdkSession, VariableInput, VariableRefusalKind, E_SDK_BAD_VARIABLE,
    E_SDK_INVALID_VARIABLES, E_SDK_PREVIEW, E_SDK_READ_ONLY, E_SDK_UNKNOWN_VARIABLE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-vars-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    if let Some(parent) = Path::new(name).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(folder.join(parent)).unwrap();
        }
    }
    std::fs::write(folder.join(name), content).unwrap();
}

fn open(folder: &Path) -> SdkSession {
    SdkSession::open(
        folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open")
}

/// Every variable type bound through one text clip: strings, color
/// (with fallback), number (quoted numeric slot), and font (through
/// the hyphenated `font-family` attribute), so the strict pass stays
/// clean.
fn bound_project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    variable id=\"count\" type=\"number\" default=1.5\n    variable id=\"show\" type=\"boolean\" default=true\n    variable id=\"size\" type=\"enum\" options=\"small, large\" default=\"large\"\n    variable id=\"brand\" type=\"font\" default=\"Family, Fallback\"\n    variable id=\"hero\" type=\"image\" default=\"media/photo.png\"\n    text value=\"var(--title) var(--show) var(--size) var(--hero)\" start=0 duration=1 x=\"var(--count)\" y=40 size=24 color=\"var(--accent, #ffffff)\" font-family=\"var(--brand)\"\n}\n",
    );
    write(folder, "media/photo.png", "\u{89}PNG\r\n\u{1a}\n");
}

fn input(id: &str, var_type: &str, default: Option<&str>) -> VariableInput {
    VariableInput {
        id: id.to_string(),
        var_type: var_type.to_string(),
        default: default.map(|s| s.to_string()),
        label: None,
        options: None,
    }
}

fn overrides(
    pairs: &[(&str, validate::validate::OverrideValue)],
) -> BTreeMap<String, validate::validate::OverrideValue> {
    let mut map = BTreeMap::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v.clone());
    }
    map
}

/// Sorted map of every file under `folder` to its bytes: the
/// nothing-partial oracle.
fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

/// Keys present after the stage but absent before: every staged or
/// persisted SDK write must land here.
fn added_keys(before: &BTreeMap<String, Vec<u8>>, folder: &Path) -> Vec<String> {
    let after = tree_bytes(folder);
    after
        .keys()
        .filter(|k| !before.contains_key(*k))
        .cloned()
        .collect()
}

/// Whether the stage added any render output (a new `mp4`/`png`/`mov`
/// and friends): source media shipped with the fixture does not
/// count, only files the SDK stage created.
fn added_render_output(before: &BTreeMap<String, Vec<u8>>, folder: &Path) -> Vec<String> {
    added_keys(before, folder)
        .into_iter()
        .filter(|k| {
            Path::new(k)
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| matches!(ext, "mp4" | "png" | "mov" | "mkv" | "webm"))
                .unwrap_or(false)
        })
        .collect()
}

fn decl_defaults(session: &SdkSession) -> Vec<String> {
    session
        .variable_declarations()
        .iter()
        .map(|d| format!("{:?}", d.default))
        .collect()
}

#[test]
fn declare_accepts_font_and_image_defaults() {
    let folder = tmp("declare-media");
    bound_project(&folder);
    let mut session = open(&folder);
    // An unknown family passes through in try order: fail-closed at
    // render, never resolved here.
    let font = session
        .declare_variable(input(
            "display",
            "font",
            Some("NoSuch Family XYZ, Fallback Sans"),
        ))
        .expect("font declares");
    assert_eq!(font.var_type, "font");
    assert_eq!(
        font.default.as_deref(),
        Some("NoSuch Family XYZ, Fallback Sans")
    );
    let image = session
        .declare_variable(input("poster", "image", Some("media/photo.png")))
        .expect("image declares");
    assert_eq!(image.var_type, "image");
    assert_eq!(image.default.as_deref(), Some("media/photo.png"));
    let ids: Vec<&str> = session
        .variable_declarations()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert!(ids.contains(&"display"));
    assert!(ids.contains(&"poster"));
    assert_eq!(session.staged_variable_records().len(), 2);
}

#[test]
fn declare_covers_every_type() {
    let folder = tmp("declare-all");
    bound_project(&folder);
    let mut session = open(&folder);
    let cases = [
        ("s", "string", Some("hello"), Some("hello")),
        ("n", "number", Some("2.5"), Some("2.5")),
        ("c", "color", Some("#ff3366"), Some("#ff3366")),
        ("b", "boolean", Some("false"), Some("false")),
        ("f", "font", Some("Alpha, Beta"), Some("Alpha, Beta")),
    ];
    for (id, ty, default, want) in cases {
        let got = session
            .declare_variable(input(id, ty, default))
            .unwrap_or_else(|e| panic!("{ty} declares: {e}"));
        assert_eq!(got.var_type, ty);
        assert_eq!(got.default.as_deref(), want, "for {ty}");
    }
    let mut sized = input("kind", "enum", Some("tall"));
    sized.options = Some("short, tall".to_string());
    let got = session.declare_variable(sized).expect("enum declares");
    assert_eq!(got.default.as_deref(), Some("tall"));
    // Default-less declarations stage too (strict validation later
    // requires a staged value for them).
    let got = session
        .declare_variable(input("later", "string", None))
        .expect("default-less declares");
    assert_eq!(got.default, None);
}

#[test]
fn declare_refuses_bad_inputs_before_anything_stages() {
    let folder = tmp("declare-bad");
    bound_project(&folder);
    let mut session = open(&folder);
    let before = session.variable_declarations().len();
    let mut bad = vec![
        input("1abc", "string", Some("x")),
        input("", "string", Some("x")),
        input("mystery", "widget", Some("x")),
        input("badnum", "number", Some("abc")),
        input("badcolor", "color", Some("red")),
        input("badbool", "boolean", Some("yes")),
        input("badfont", "font", Some("")),
        input("badfont2", "font", Some(" , ")),
        input("badimage", "image", Some("media/missing.png")),
        // `title` is declared on disk: duplicates refuse.
        input("title", "string", Some("dup")),
        // Only `enum` holds `options`.
        VariableInput {
            id: "withopts".to_string(),
            var_type: "string".to_string(),
            default: Some("x".to_string()),
            label: None,
            options: Some("a, b".to_string()),
        },
        // `enum` needs at least two options.
        VariableInput {
            id: "lonely".to_string(),
            var_type: "enum".to_string(),
            default: Some("a".to_string()),
            label: None,
            options: Some("only".to_string()),
        },
    ];
    // An `enum` default outside `options` refuses.
    bad.push(VariableInput {
        id: "offmenu".to_string(),
        var_type: "enum".to_string(),
        default: Some("third".to_string()),
        label: None,
        options: Some("first, second".to_string()),
    });
    for candidate in bad {
        let id = candidate.id.clone();
        let err = match session.declare_variable(candidate) {
            Ok(_) => panic!("{id} must refuse"),
            Err(e) => e,
        };
        assert_eq!(err.code, E_SDK_BAD_VARIABLE, "for {id}");
        assert_eq!(err.kind, VariableRefusalKind::BadVariable, "for {id}");
    }
    // A staged id refuses its duplicate too.
    session
        .declare_variable(input("fresh", "string", Some("one")))
        .expect("first stages");
    let err = session
        .declare_variable(input("fresh", "string", Some("two")))
        .expect_err("staged duplicate refuses");
    assert_eq!(err.code, E_SDK_BAD_VARIABLE);
    assert_eq!(session.variable_declarations().len(), before + 1);
    assert_eq!(session.staged_variable_records().len(), 1);
}

#[test]
fn validate_reports_every_offender_with_rule_and_location() {
    let folder = tmp("validate-report");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"unused-a\" type=\"string\" default=\"kept\"\n    variable id=\"unused-b\" type=\"number\" default=1.5\n    text value=\"var(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let session = open(&folder);
    let offenders = session.validate_variables();
    assert_eq!(offenders.len(), 2, "both unused declarations report");
    assert_eq!(offenders[0].rule, "E_BAD_VARIABLE");
    assert!(offenders[0].file.ends_with("composition.kdl"));
    assert_eq!(offenders[0].line, 3);
    assert!(offenders[0].message.contains("--unused-a"));
    assert!(!offenders[0].fix.is_empty());
    assert_eq!(offenders[1].rule, "E_BAD_VARIABLE");
    assert_eq!(offenders[1].line, 4);
    assert!(offenders[1].message.contains("--unused-b"));
    // A clean project validates to nothing.
    let clean = tmp("validate-clean");
    bound_project(&clean);
    let clean_session = open(&clean);
    assert!(clean_session.validate_variables().is_empty());
}

#[test]
fn usage_scan_lists_every_reference() {
    let folder = tmp("scan");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    variable id=\"subtitle\" type=\"string\" default=\"Sub\"\n    text value=\"Hello var(--title)!\" start=0 duration=1 x=10 y=40 size=24 color=\"var(--accent, #ffffff)\"\n    text value=\"Fallback var(--title, var(--subtitle))\" start=0 duration=1 x=10 y=80 size=24 color=\"#ffffff\"\n}\n",
    );
    let session = open(&folder);
    let uses = session.scan_variable_usage(None);
    let compact: Vec<(&str, usize, usize, &str)> = uses
        .iter()
        .map(|u| (u.variable.as_str(), u.line, u.column, u.attribute.as_str()))
        .collect();
    assert_eq!(
        compact,
        vec![
            ("title", 5, 23, "value"),
            ("accent", 5, 82, "color"),
            ("title", 6, 26, "value"),
            ("subtitle", 6, 39, "value"),
        ]
    );
    for u in &uses {
        assert!(u.file.ends_with("composition.kdl"));
    }
}

#[test]
fn usage_scan_filter_selects_one_variable() {
    let folder = tmp("scan-filter");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    text value=\"var(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"var(--accent, #ffffff)\"\n}\n",
    );
    let session = open(&folder);
    let uses = session.scan_variable_usage(Some("accent"));
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].variable, "accent");
    assert_eq!(uses[0].attribute, "color");
    assert!(session.scan_variable_usage(Some("missing")).is_empty());
}

#[test]
fn usage_scan_ignores_lookalikes_and_malformed_calls() {
    let folder = tmp("scan-edge");
    std::fs::create_dir_all(&folder).unwrap();
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"myvar(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let found = sdk::variables::scan_folder(&folder);
    assert!(found.is_empty(), "myvar( is no call: {found:?}");
}

#[test]
fn override_deltas_apply_without_touching_declared_defaults() {
    let folder = tmp("overrides-apply");
    bound_project(&folder);
    let mut session = open(&folder);
    let before = decl_defaults(&session);
    let before_len = session.variable_declarations().len();
    let report = session
        .apply_variable_overrides(overrides(&[
            (
                "title",
                validate::validate::OverrideValue::Str("Sale".to_string()),
            ),
            (
                "accent",
                validate::validate::OverrideValue::Str("#00ff00".to_string()),
            ),
            ("count", validate::validate::OverrideValue::Num(3.0)),
            ("show", validate::validate::OverrideValue::Bool(false)),
            (
                "size",
                validate::validate::OverrideValue::Str("small".to_string()),
            ),
            (
                "brand",
                validate::validate::OverrideValue::Str("New Face, Other".to_string()),
            ),
            (
                "hero",
                validate::validate::OverrideValue::Str("media/photo.png".to_string()),
            ),
        ]))
        .expect("typed delta applies");
    assert_eq!(
        report.applied,
        vec!["accent", "brand", "count", "hero", "show", "size", "title"]
    );
    // Declared defaults are untouched: overrides resolve at bind
    // time, never by rewriting declarations.
    assert_eq!(decl_defaults(&session), before);
    assert_eq!(session.variable_declarations().len(), before_len);
    let merged = session.variable_overrides();
    assert_eq!(
        merged.get("title"),
        Some(&validate::validate::OverrideValue::Str("Sale".to_string()))
    );
    assert_eq!(
        merged.get("brand"),
        Some(&validate::validate::OverrideValue::Str(
            "New Face, Other".to_string()
        ))
    );
    assert_eq!(session.staged_variable_overrides().len(), 7);
}

#[test]
fn undeclared_overrides_refuse_without_creating() {
    let folder = tmp("overrides-undeclared");
    bound_project(&folder);
    let mut session = open(&folder);
    let before = session.variable_declarations().len();
    let err = session
        .apply_variable_overrides(overrides(&[
            (
                "title",
                validate::validate::OverrideValue::Str("Sale".to_string()),
            ),
            (
                "ghost",
                validate::validate::OverrideValue::Str("boo".to_string()),
            ),
        ]))
        .expect_err("undeclared key refuses the whole delta");
    assert_eq!(err.code, E_SDK_UNKNOWN_VARIABLE);
    assert_eq!(err.kind, VariableRefusalKind::UnknownVariable);
    assert!(err.message.contains("--ghost"), "names it: {err}");
    // Nothing staged, nothing created.
    assert!(session.staged_variable_overrides().is_empty());
    assert_eq!(session.variable_declarations().len(), before);
    assert!(!session.variable_overrides().contains_key("ghost"));
}

#[test]
fn mistyped_overrides_refuse_without_staging() {
    let folder = tmp("overrides-mistyped");
    bound_project(&folder);
    let mut session = open(&folder);
    let cases: Vec<(&str, Vec<(&str, validate::validate::OverrideValue)>)> = vec![
        (
            "number",
            vec![(
                "count",
                validate::validate::OverrideValue::Str("many".to_string()),
            )],
        ),
        (
            "color",
            vec![("accent", validate::validate::OverrideValue::Num(3.0))],
        ),
        (
            "boolean",
            vec![("show", validate::validate::OverrideValue::Num(1.0))],
        ),
        (
            "enum",
            vec![(
                "size",
                validate::validate::OverrideValue::Str("huge".to_string()),
            )],
        ),
        (
            "font",
            vec![("brand", validate::validate::OverrideValue::Bool(true))],
        ),
        (
            "image-missing",
            vec![(
                "hero",
                validate::validate::OverrideValue::Str("media/missing.png".to_string()),
            )],
        ),
    ];
    for (tag, pairs) in cases {
        let err = match session.apply_variable_overrides(overrides(&pairs)) {
            Ok(_) => panic!("{tag} must refuse"),
            Err(e) => e,
        };
        assert_eq!(err.code, E_SDK_BAD_VARIABLE, "for {tag}");
    }
    assert!(session.staged_variable_overrides().is_empty());
    assert!(session.validate_variables().is_empty());
}

#[test]
fn failed_validation_blocks_persistence_with_nothing_saved() {
    let folder = tmp("persist-blocked");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"unused\" type=\"string\" default=\"kept\"\n    text value=\"var(--title)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    let err = session
        .persist_variables()
        .expect_err("unused variable blocks persistence");
    assert_eq!(err.code, E_SDK_INVALID_VARIABLES);
    assert_eq!(err.kind, sdk::VariableRefusalKind::Invalid);
    assert!(err.message.contains("E_BAD_VARIABLE"), "lists rules: {err}");
    assert!(err.message.contains("--unused"), "lists offenders: {err}");
    // Nothing partial: no delta file, sources byte-identical.
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn stale_staged_values_block_persistence_with_nothing_saved() {
    let folder = tmp("persist-stale");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    text value=\"var(--hero, none)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    write(&folder, "media/photo.png", "\u{89}PNG\r\n\u{1a}\n");
    let mut session = open(&folder);
    session
        .declare_variable(input("hero", "image", Some("media/photo.png")))
        .expect("declares while the file reads");
    session
        .apply_variable_overrides(overrides(&[(
            "hero",
            validate::validate::OverrideValue::Str("media/photo.png".to_string()),
        )]))
        .expect("applies while the file reads");
    assert!(session.validate_variables().is_empty());
    std::fs::remove_file(folder.join("media/photo.png")).unwrap();
    let offenders = session.validate_variables();
    assert!(
        offenders.iter().any(|o| o.message.contains("--hero")),
        "stale image reports: {offenders:?}"
    );
    let before = tree_bytes(&folder);
    let err = session
        .persist_variables()
        .expect_err("stale staged value blocks persistence");
    assert_eq!(err.code, E_SDK_INVALID_VARIABLES);
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn declare_and_apply_stage_in_memory_only() {
    let folder = tmp("stage-memory");
    bound_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = open(&folder);
    session
        .declare_variable(input("tagline", "string", Some("Fresh")))
        .expect("declares");
    session
        .apply_variable_overrides(overrides(&[(
            "title",
            validate::validate::OverrideValue::Str("Sale".to_string()),
        )]))
        .expect("applies");
    assert_eq!(tree_bytes(&folder), before);
    assert!(added_render_output(&before, &folder).is_empty());
}

#[test]
fn persist_writes_hidden_deltas_and_replays() {
    let folder = tmp("persist-replay");
    write(
        &folder,
        "composition.kdl",
        "composition name=\"t\" width=320 height=180 duration=1 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    text value=\"var(--title) var(--tagline, Draft)\" start=0 duration=1 x=10 y=40 size=24 color=\"#ffffff\"\n}\n",
    );
    let kdl_before = std::fs::read(folder.join("composition.kdl")).unwrap();
    let before_tree = tree_bytes(&folder);
    let mut session = open(&folder);
    session
        .declare_variable(input("tagline", "string", Some("Fresh")))
        .expect("declares");
    session
        .apply_variable_overrides(overrides(&[
            (
                "title",
                validate::validate::OverrideValue::Str("Sale".to_string()),
            ),
            (
                "tagline",
                validate::validate::OverrideValue::Str("Live".to_string()),
            ),
        ]))
        .expect("applies");
    let report = session.persist_variables().expect("persists");
    assert_eq!(report.declarations, 1);
    assert_eq!(report.overrides, 2);
    assert_eq!(report.paths.len(), 2);
    // Only the hidden directory changed: sources byte-identical, no
    // render output, default renders unaffected.
    assert_eq!(
        std::fs::read(folder.join("composition.kdl")).unwrap(),
        kdl_before
    );
    for key in added_keys(&before_tree, &folder) {
        assert!(
            key.starts_with(".framesmith/"),
            "persist writes hidden deltas only, found {key}"
        );
    }
    assert!(added_render_output(&before_tree, &folder).is_empty());
    assert!(folder.join(".framesmith/sdk-variables.json").is_file());
    assert!(folder
        .join(".framesmith/sdk-variable-overrides.json")
        .is_file());
    // Persisting a clean stage writes nothing.
    let clean = session.persist_variables().expect("clean persist");
    assert_eq!(clean.declarations, 0);
    assert_eq!(clean.overrides, 0);
    assert!(clean.paths.is_empty());
    // Reopening replays both deltas.
    let reopened = open(&folder);
    let ids: Vec<&str> = reopened
        .variable_declarations()
        .iter()
        .map(|d| d.id.as_str())
        .collect();
    assert!(ids.contains(&"tagline"));
    assert_eq!(reopened.staged_variable_records().len(), 0);
    let merged = reopened.variable_overrides();
    assert_eq!(
        merged.get("tagline"),
        Some(&validate::validate::OverrideValue::Str("Live".to_string()))
    );
    assert_eq!(
        merged.get("title"),
        Some(&validate::validate::OverrideValue::Str("Sale".to_string()))
    );
    // Declared defaults still read as declared (overrides never
    // rewrote them).
    let title = reopened
        .variable_declarations()
        .iter()
        .find(|d| d.id == "title")
        .expect("title declared");
    assert_eq!(format!("{:?}", title.default), "Some(Str(\"News\"))");
    assert!(reopened.validate_variables().is_empty());
}

#[test]
fn read_only_sessions_refuse_staging() {
    let folder = tmp("readonly");
    bound_project(&folder);
    let mut session = SdkSession::open(
        &folder,
        OpenOptions {
            persist: false,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    let err = session
        .declare_variable(input("tagline", "string", Some("Fresh")))
        .expect_err("read-only declare refuses");
    assert_eq!(err.code, E_SDK_READ_ONLY);
    let err = session
        .apply_variable_overrides(overrides(&[(
            "title",
            validate::validate::OverrideValue::Str("Sale".to_string()),
        )]))
        .expect_err("read-only apply refuses");
    assert_eq!(err.code, E_SDK_READ_ONLY);
}

#[test]
fn preview_sessions_stage_but_never_persist() {
    let folder = tmp("preview");
    bound_project(&folder);
    let before = tree_bytes(&folder);
    let mut session = SdkSession::open(
        &folder,
        OpenOptions {
            preview: true,
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open");
    session
        .declare_variable(input("tagline", "string", Some("Fresh")))
        .expect("preview stages declarations");
    session
        .apply_variable_overrides(overrides(&[(
            "title",
            validate::validate::OverrideValue::Str("Sale".to_string()),
        )]))
        .expect("preview stages overrides");
    let err = session
        .persist_variables()
        .expect_err("preview never persists");
    assert_eq!(err.code, E_SDK_PREVIEW);
    assert_eq!(tree_bytes(&folder), before);
}

#[test]
fn corrupt_variable_delta_refuses_open_naming_the_file() {
    let folder = tmp("corrupt-delta");
    bound_project(&folder);
    std::fs::create_dir_all(folder.join(".framesmith")).unwrap();
    write(&folder, ".framesmith/sdk-variables.json", "{not json");
    let err = SdkSession::open(
        &folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect_err("corrupt delta refuses the open");
    assert!(err.message.contains("sdk-variables.json"), "{err}");
}
