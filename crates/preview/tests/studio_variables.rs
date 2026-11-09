//! Variables panel regressions (Sprint 18, T005 / CAP-005, J-02,
//! STATE-02, INT-001, REC-001): override previews leave declared
//! defaults untouched with nothing written; declare, edit, and
//! remove validate like the SDK and record guarded history turns;
//! binding writes the reference onto the selected element as a
//! validated turn; copies place exact values text and a runnable
//! render command; unused and undeclared warnings list on the
//! panel; in-use removals and undeclared binds refuse by name.

use preview::daemon;
use preview::server::{run_serve, ServeOptions};
use preview::{studio, studio_variables};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-studio-variables-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn project(folder: &Path) {
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=360 duration=6 fps=10 background=\"#112233\" version=1 {\n    variable id=\"title\" type=\"string\" default=\"News\"\n    variable id=\"accent\" type=\"color\" default=\"#ff3366\"\n    variable id=\"lonely\" type=\"string\" default=\"L\"\n    text id=\"headline\" value=\"Hello\" start=0 duration=6 x=10 y=40 size=24 color=\"var(--accent, #ffffff)\"\n    text id=\"bound\" value=\"var(--title)\" start=0 duration=6 x=10 y=120 size=24 color=\"#ffffff\"\n}\n",
    );
}

fn input(id: &str, var_type: &str, default: Option<&str>) -> sdk::VariableInput {
    sdk::VariableInput {
        id: id.to_string(),
        var_type: var_type.to_string(),
        default: default.map(|s| s.to_string()),
        label: None,
        options: None,
    }
}

fn delta(
    pairs: &[(&str, validate::validate::OverrideValue)],
) -> validate::validate::VariableOverrides {
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

fn kdl_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    tree_bytes(folder)
        .into_iter()
        .filter(|(k, _)| k.ends_with(".kdl") && !k.starts_with('.'))
        .collect()
}

fn turn_count(folder: &Path) -> usize {
    sdk::history::list_turns(folder)
        .expect("history lists")
        .turns
        .len()
}

fn shown(folder: &Path) -> notation::model::Composition {
    studio::effective_composition_for_session(folder, Some("t")).expect("effective comp")
}

fn live_value(folder: &Path, clip_id: &str, property: &str) -> Option<String> {
    let comp = shown(folder);
    let clip = comp.clips.iter().find(|c| c.id() == Some(clip_id))?;
    sdk::row_live_value(clip, property)
}

/// Override previews stage in memory and drop: declared defaults
/// read back untouched, nothing is written anywhere — not even
/// the hidden history directory.
#[test]
fn preview_leaves_defaults_untouched() {
    let dir = tmp("preview");
    project(&dir);
    let before_tree = tree_bytes(&dir);
    assert!(
        !dir.join(".framesmith").exists(),
        "fixture starts with no hidden state"
    );
    let empty = studio_variables::preview_variable_overrides(&dir, Some("t"), delta(&[]))
        .expect("empty preview");
    assert_eq!(empty.composition, "t");
    assert!(empty.applied.is_empty());
    assert_eq!(empty.values, "{}");
    assert!(empty.warnings.is_empty());
    let defaults_before: Vec<(String, Option<String>)> = empty
        .defaults
        .iter()
        .map(|d| (d.id.clone(), d.default.clone()))
        .collect();
    assert_eq!(
        defaults_before,
        vec![
            ("title".to_string(), Some("News".to_string())),
            ("accent".to_string(), Some("#ff3366".to_string())),
            ("lonely".to_string(), Some("L".to_string())),
        ],
        "defaults read back: {defaults_before:?}"
    );
    let preview = studio_variables::preview_variable_overrides(
        &dir,
        Some("t"),
        delta(&[
            (
                "title",
                validate::validate::OverrideValue::Str("Flash".to_string()),
            ),
            (
                "accent",
                validate::validate::OverrideValue::Str("#00ff00".to_string()),
            ),
        ]),
    )
    .expect("preview applies");
    assert_eq!(
        preview.applied,
        vec!["accent".to_string(), "title".to_string()]
    );
    assert_eq!(
        preview.values,
        "{\"accent\":\"#00ff00\",\"title\":\"Flash\"}"
    );
    let defaults_after: Vec<(String, Option<String>)> = preview
        .defaults
        .iter()
        .map(|d| (d.id.clone(), d.default.clone()))
        .collect();
    assert_eq!(defaults_after, defaults_before, "defaults untouched");
    assert!(preview.warnings.is_empty());
    assert_eq!(tree_bytes(&dir), before_tree, "preview writes nothing");
}

/// Unknown preview keys warn naming the key — never silently
/// created — while known keys still apply; mistyped values
/// refuse the whole preview staging nothing.
#[test]
fn preview_warns_unknown_and_refuses_mistyped() {
    let dir = tmp("preview-warn");
    project(&dir);
    let before_tree = tree_bytes(&dir);
    let preview = studio_variables::preview_variable_overrides(
        &dir,
        Some("t"),
        delta(&[
            (
                "title",
                validate::validate::OverrideValue::Str("Flash".to_string()),
            ),
            ("ghost", validate::validate::OverrideValue::Num(1.0)),
        ]),
    )
    .expect("preview with unknown key warns");
    assert_eq!(preview.applied, vec!["title".to_string()]);
    assert_eq!(preview.values, "{\"title\":\"Flash\"}");
    assert_eq!(
        preview.warnings.len(),
        1,
        "one warning: {:?}",
        preview.warnings
    );
    assert_eq!(
        preview.warnings[0].kind,
        studio_variables::PanelWarningKind::Undeclared
    );
    assert_eq!(preview.warnings[0].variable.as_deref(), Some("ghost"));
    assert_eq!(tree_bytes(&dir), before_tree, "preview writes nothing");
    let refused = studio_variables::preview_variable_overrides(
        &dir,
        Some("t"),
        delta(&[(
            "accent",
            validate::validate::OverrideValue::Str("notacolor".to_string()),
        )]),
    )
    .expect_err("mistyped preview refuses");
    assert!(
        refused.contains("E_SDK_BAD_VARIABLE"),
        "names the rule: {refused}"
    );
    assert_eq!(tree_bytes(&dir), before_tree, "refusal writes nothing");
}

/// Declare, edit, and remove validate like the SDK and record one
/// guarded history turn each: bad rules refuse staging nothing,
/// unknown and on-disk ids refuse, and undo reverses a declare
/// whole.
#[test]
fn declare_edit_remove_record_guarded_turns() {
    let dir = tmp("turns");
    project(&dir);
    assert_eq!(turn_count(&dir), 0);
    let receipt = studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("fresh", "string", Some("Hi")),
    )
    .expect("declare");
    assert_eq!(receipt.declared.id, "fresh");
    assert_eq!(receipt.declared.var_type, "string");
    assert_eq!(receipt.declared.default.as_deref(), Some("Hi"));
    assert_eq!(receipt.turn, 0, "first turn: {}", receipt.turn);
    assert_eq!(turn_count(&dir), 1);
    assert_eq!(
        receipt.warnings.len(),
        2,
        "staged-unused plus disk-health: {:?}",
        receipt.warnings
    );
    assert!(
        receipt
            .warnings
            .iter()
            .any(|o| o.message.contains("--fresh")),
        "staged advisory names it: {:?}",
        receipt.warnings
    );
    assert!(
        receipt
            .warnings
            .iter()
            .any(|o| o.message.contains("--lonely")),
        "pre-existing disk health lists, never blocks: {:?}",
        receipt.warnings
    );
    assert!(
        dir.join(".framesmith/sdk-variables.json").exists(),
        "delta persisted in hidden state"
    );
    let bad = studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("bad", "nope", Some("Hi")),
    )
    .expect_err("bad type refuses");
    assert!(bad.contains("E_SDK_BAD_VARIABLE"), "names the rule: {bad}");
    assert_eq!(turn_count(&dir), 1, "refusal records no turn");
    let dup = studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("title", "string", Some("Hi")),
    )
    .expect_err("duplicate refuses");
    assert!(dup.contains("E_SDK_BAD_VARIABLE"), "names the rule: {dup}");
    let edited = studio_variables::edit_panel_variable(
        &dir,
        Some("t"),
        "maker",
        "fresh",
        input("fresh", "string", Some("Yo")),
    )
    .expect("edit");
    assert_eq!(edited.declared.default.as_deref(), Some("Yo"));
    assert_eq!(turn_count(&dir), 2);
    let comp = shown(&dir);
    let fresh = comp
        .variables
        .iter()
        .find(|d| d.id == "fresh")
        .expect("fresh staged");
    assert_eq!(
        format!("{:?}", fresh.default),
        format!(
            "{:?}",
            Some(notation::model::VariableDefault::Str("Yo".to_string()))
        ),
        "edit replays"
    );
    let bad_edit = studio_variables::edit_panel_variable(
        &dir,
        Some("t"),
        "maker",
        "fresh",
        input("fresh", "number", Some("abc")),
    )
    .expect_err("mistyped edit refuses");
    assert!(
        bad_edit.contains("E_SDK_BAD_VARIABLE"),
        "names the rule: {bad_edit}"
    );
    assert_eq!(turn_count(&dir), 2, "refusal records no turn");
    let unknown_edit = studio_variables::edit_panel_variable(
        &dir,
        Some("t"),
        "maker",
        "ghost",
        input("ghost", "string", Some("A")),
    )
    .expect_err("unknown edit refuses");
    assert!(
        unknown_edit.contains("E_SDK_UNKNOWN_VARIABLE"),
        "names the code: {unknown_edit}"
    );
    assert!(unknown_edit.contains("--ghost"), "names it: {unknown_edit}");
    let disk_edit = studio_variables::edit_panel_variable(
        &dir,
        Some("t"),
        "maker",
        "title",
        input("title", "string", Some("Zed")),
    )
    .expect_err("on-disk edit refuses");
    assert!(
        disk_edit.contains("project sources"),
        "routes to source: {disk_edit}"
    );
    let removed = studio_variables::remove_panel_variable(&dir, Some("t"), "maker", "fresh")
        .expect("remove unused");
    assert_eq!(removed.id, "fresh");
    assert_eq!(turn_count(&dir), 3);
    assert!(
        shown(&dir).variables.iter().all(|d| d.id != "fresh"),
        "removal replays"
    );
    sdk::history::undo_turn(&dir, removed.turn).expect("undo reverses the removal");
    assert!(
        shown(&dir).variables.iter().any(|d| d.id == "fresh"),
        "undo restores the declaration"
    );
}

/// Binding writes the reference onto the selected element as one
/// validated turn: the row reads `var(--id)`, the turn undoes
/// clean, and undeclared targets refuse naming the id without
/// creating anything.
#[test]
fn bind_writes_reference_as_validated_turn() {
    let dir = tmp("bind");
    project(&dir);
    let bind = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"value","variable":"title","selection":"headline,bound"}"#,
    )
    .expect("bind parses");
    assert_eq!(bind.property, "value");
    assert_eq!(bind.variable, "title");
    assert_eq!(
        bind.selection,
        vec!["headline".to_string(), "bound".to_string()]
    );
    let receipt =
        studio_variables::apply_variable_bind(&dir, Some("t"), &bind).expect("bind applies");
    assert_eq!(receipt.applied, 1);
    assert_eq!(turn_count(&dir), 1);
    assert_eq!(
        live_value(&dir, "headline", "value").as_deref(),
        Some("var(--title)"),
        "reference written onto the row"
    );
    let refs = studio_variables::panel_references(&dir, Some("t"), Some("title"))
        .expect("references list");
    assert!(
        refs.iter()
            .any(|r| r.location.contains("clip \"headline\"") && r.attribute == "value"),
        "staged-layer reference lists: {refs:?}"
    );
    sdk::history::undo_turn(&dir, receipt.turn).expect("undo reverses the bind");
    assert_eq!(
        live_value(&dir, "headline", "value").as_deref(),
        Some("Hello"),
        "undo restores the prior value"
    );
    let ghost = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"value","variable":"--ghost"}"#,
    )
    .expect("ghost parses (-- prefix accepted)");
    let refused = studio_variables::apply_variable_bind(&dir, Some("t"), &ghost)
        .expect_err("undeclared bind refuses");
    assert!(refused.contains("--ghost"), "refusal names it: {refused}");
    assert!(
        refused.contains("never created"),
        "never created silently: {refused}"
    );
    assert_eq!(turn_count(&dir), 1, "refusal records no turn");
    assert!(
        studio_variables::panel_references(&dir, Some("t"), Some("ghost"))
            .expect("references list")
            .is_empty(),
        "nothing created"
    );
    let typed = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"font-size","variable":"title"}"#,
    )
    .expect_err("typed rows refuse at parse");
    assert!(
        typed.contains("text-carrying"),
        "names the boundary: {typed}"
    );
    let missing = studio_variables::VariableBind {
        actor: "maker".to_string(),
        target: sdk::ClipRef::by_id("ghost-clip"),
        property: "value".to_string(),
        variable: "title".to_string(),
        selection: Vec::new(),
    };
    let unknown = studio_variables::apply_variable_bind(&dir, Some("t"), &missing)
        .expect_err("missing target refuses");
    assert!(
        unknown.contains("ghost-clip"),
        "names the target: {unknown}"
    );
}

/// Declaring then binding sequences through the panel: a font
/// variable binds onto `font-family` verbatim (fail-closed at
/// render time, never resolved here).
#[test]
fn declare_then_bind_font_family() {
    let dir = tmp("bind-font");
    project(&dir);
    let declared = studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("brand", "font", Some("Family, Fallback")),
    )
    .expect("declare font");
    assert_eq!(declared.declared.var_type, "font");
    assert_eq!(
        declared.declared.default.as_deref(),
        Some("Family, Fallback")
    );
    let bind = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"font-family","variable":"brand"}"#,
    )
    .expect("bind parses");
    studio_variables::apply_variable_bind(&dir, Some("t"), &bind).expect("bind applies");
    assert_eq!(
        live_value(&dir, "headline", "font-family").as_deref(),
        Some("var(--brand)"),
        "reference rides verbatim"
    );
}

/// Copies place exact values text and a runnable render command:
/// the text parses back to the identical map through the exact
/// CLI payload parser, the command carries folder, output
/// outside the sources, variables, and composition, and its
/// payload parses back identical too.
#[test]
fn copies_place_exact_values_and_runnable_command() {
    let dir = tmp("copies");
    project(&dir);
    let map = delta(&[
        (
            "title",
            validate::validate::OverrideValue::Str("Flash".to_string()),
        ),
        ("count", validate::validate::OverrideValue::Num(1.5)),
        ("show", validate::validate::OverrideValue::Bool(true)),
    ]);
    let values = studio_variables::copy_variable_values(&map);
    assert_eq!(values, "{\"count\":1.5,\"show\":true,\"title\":\"Flash\"}");
    let back =
        validate::validate::parse_variable_overrides(&values).expect("values parse like CLI");
    assert_eq!(back, map, "exact round-trip");
    assert_eq!(studio_variables::copy_variable_values(&delta(&[])), "{}");
    let command = studio_variables::copy_render_command(&dir, Some("t"), &map, None);
    assert!(
        command.starts_with("framesmith render "),
        "command: {command}"
    );
    assert!(command.contains("--variables"), "command: {command}");
    assert!(command.contains("--composition"), "command: {command}");
    assert!(
        command.contains(&format!("'{}'", dir.display())),
        "folder quoted: {command}"
    );
    assert!(
        command.contains(&format!("'{values}'")),
        "exact values quoted: {command}"
    );
    let out_flag = command.find("-o ").expect("output flag");
    let out_text = &command[out_flag..];
    assert!(
        !out_text.starts_with(&format!("-o '{}/", dir.display())),
        "output lands outside the project sources: {command}"
    );
    let payload = command
        .split("--variables '")
        .nth(1)
        .and_then(|tail| tail.split('\'').next())
        .expect("quoted payload");
    let back =
        validate::validate::parse_variable_overrides(payload).expect("payload parses like CLI");
    assert_eq!(back, map, "command replays the previewed overrides");
    let explicit = studio_variables::copy_render_command(&dir, None, &map, Some("/tmp/out.mp4"));
    assert!(
        explicit.contains("-o '/tmp/out.mp4'"),
        "explicit output: {explicit}"
    );
    assert!(
        !explicit.contains("--composition"),
        "no composition flag: {explicit}"
    );
}

/// Unused and undeclared warnings list on the panel: declared
/// ids with no binding warn as unused, and binding one clears
/// its warning.
#[test]
fn warnings_list_unused_and_undeclared() {
    let dir = tmp("warnings");
    project(&dir);
    let warnings = studio_variables::panel_warnings(&dir, Some("t")).expect("warnings list");
    let unused: Vec<String> = warnings
        .iter()
        .filter(|w| w.kind == studio_variables::PanelWarningKind::Unused)
        .filter_map(|w| w.variable.clone())
        .collect();
    assert_eq!(
        unused,
        vec!["lonely".to_string()],
        "unused lists: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .all(|w| w.kind != studio_variables::PanelWarningKind::Undeclared),
        "no undeclared in a sound project: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .all(|w| w.kind != studio_variables::PanelWarningKind::Invalid),
        "no invalid in a sound project: {warnings:?}"
    );
    let listing = studio_variables::variables_json(&dir, Some("t")).expect("listing renders");
    assert!(
        listing.contains("\"id\":\"lonely\""),
        "lonely lists: {listing}"
    );
    assert!(
        listing.contains("\"bound\":false"),
        "unbound marks: {listing}"
    );
    assert!(listing.contains("\"bound\":true"), "bound marks: {listing}");
    assert!(
        listing.contains("\"kind\":\"unused\""),
        "warnings render: {listing}"
    );
    let bind = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"value","variable":"lonely"}"#,
    )
    .expect("bind parses");
    studio_variables::apply_variable_bind(&dir, Some("t"), &bind).expect("bind applies");
    let warnings = studio_variables::panel_warnings(&dir, Some("t")).expect("warnings re-list");
    assert!(
        warnings
            .iter()
            .all(|w| w.variable.as_deref() != Some("lonely")),
        "binding clears the warning: {warnings:?}"
    );
}

/// In-use removals refuse by name with every site listed — for
/// staged and on-disk declarations alike — and unknown removals
/// name `E_SDK_UNKNOWN_VARIABLE`; refusals stage nothing.
#[test]
fn in_use_removal_refuses_by_name() {
    let dir = tmp("refusals");
    project(&dir);
    studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("stage", "string", Some("S")),
    )
    .expect("declare stage");
    let bind = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"value","variable":"stage"}"#,
    )
    .expect("bind parses");
    studio_variables::apply_variable_bind(&dir, Some("t"), &bind).expect("bind applies");
    let before_turns = turn_count(&dir);
    let refused = studio_variables::remove_panel_variable(&dir, Some("t"), "maker", "stage")
        .expect_err("staged in-use removal refuses");
    assert!(refused.contains("--stage"), "refusal names it: {refused}");
    assert!(
        refused.contains("row `value`"),
        "lists the staged site: {refused}"
    );
    let disk_refused = studio_variables::remove_panel_variable(&dir, Some("t"), "maker", "title")
        .expect_err("on-disk in-use removal refuses");
    assert!(
        disk_refused.contains("--title"),
        "refusal names it: {disk_refused}"
    );
    assert!(
        disk_refused.contains("composition.kdl"),
        "lists the source site: {disk_refused}"
    );
    let unknown = studio_variables::remove_panel_variable(&dir, Some("t"), "maker", "ghost")
        .expect_err("unknown removal refuses");
    assert!(
        unknown.contains("E_SDK_UNKNOWN_VARIABLE"),
        "names the code: {unknown}"
    );
    assert!(unknown.contains("--ghost"), "names it: {unknown}");
    assert_eq!(turn_count(&dir), before_turns, "refusals record no turns");
}

/// Panel work writes validated turns plus hidden studio state
/// only: composition sources stay byte-identical and no render
/// output appears anywhere.
#[test]
fn panel_work_keeps_sources_byte_identical() {
    let dir = tmp("identical");
    project(&dir);
    let before_kdl = kdl_bytes(&dir);
    studio_variables::declare_panel_variable(
        &dir,
        Some("t"),
        "maker",
        input("fresh", "string", Some("Hi")),
    )
    .expect("declare");
    studio_variables::edit_panel_variable(
        &dir,
        Some("t"),
        "maker",
        "fresh",
        input("fresh", "string", Some("Yo")),
    )
    .expect("edit");
    let bind = studio_variables::parse_bind_body(
        r#"{"actor":"maker","target":{"id":"headline"},"property":"value","variable":"fresh"}"#,
    )
    .expect("bind parses");
    studio_variables::apply_variable_bind(&dir, Some("t"), &bind).expect("bind applies");
    studio_variables::preview_variable_overrides(&dir, Some("t"), delta(&[])).expect("preview");
    let _ = studio_variables::panel_warnings(&dir, Some("t")).expect("warnings");
    let _ = studio_variables::variables_json(&dir, Some("t")).expect("listing");
    studio_variables::remove_panel_variable(&dir, Some("t"), "maker", "fresh")
        .expect_err("bound removal refuses");
    assert_eq!(kdl_bytes(&dir), before_kdl, "sources byte-identical");
    for (name, _) in tree_bytes(&dir) {
        assert!(
            !name.ends_with(".mp4") && !name.ends_with(".png") && !name.ends_with(".mov"),
            "no render output: {name}"
        );
        assert!(
            name.ends_with(".kdl") || name.starts_with(".framesmith/"),
            "only sources plus hidden state: {name}"
        );
    }
}

/// Malformed panel bodies refuse naming their cause and staging
/// nothing.
#[test]
fn malformed_bodies_refuse() {
    assert!(studio_variables::parse_bind_body("").is_err(), "empty bind");
    assert!(
        studio_variables::parse_bind_body("{nope").is_err(),
        "bad JSON"
    );
    assert!(
        studio_variables::parse_bind_body(
            r#"{"target":{"id":"a"},"property":"value","variable":"v"}"#
        )
        .expect_err("no actor")
        .contains("actor"),
        "actor named"
    );
    assert!(
        studio_variables::parse_bind_body(
            r#"{"actor":"m","target":{"id":"a"},"property":"value"}"#
        )
        .expect_err("no variable")
        .contains("variable"),
        "variable named"
    );
    assert!(
        studio_variables::parse_bind_body(
            r#"{"actor":"m","target":{"id":"a"},"property":"value","variable":"9bad"}"#
        )
        .expect_err("bad id")
        .contains("variable id"),
        "id rule named"
    );
    assert!(
        studio_variables::parse_variables_action(r#"{"action":"frobnicate"}"#)
            .expect_err("unknown action")
            .contains("declare"),
        "actions named"
    );
    assert!(
        studio_variables::parse_variables_action(r#"{"action":"preview","overrides":{"a":[1]}}"#)
            .expect_err("non-scalar overrides")
            .contains("overrides"),
        "overrides named"
    );
    let preview = studio_variables::parse_variables_action(
        r#"{"action":"preview","overrides":{"title":"Flash"}}"#,
    )
    .expect("preview parses like CLI");
    match preview {
        studio_variables::VariablesAction::Preview { delta } => {
            assert_eq!(
                studio_variables::copy_variable_values(&delta),
                "{\"title\":\"Flash\"}"
            );
        }
        other => panic!("wrong action: {other:?}"),
    }
}

/// Minimal HTTP exchange over loopback with an optional body.
fn http(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, Vec<u8>) {
    let addr = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(5))
        .expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {target} HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let code = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    (code, raw)
}

fn body_str(port: u16, method: &str, target: &str, body: Option<&str>) -> (u16, String) {
    let (code, raw) = http(port, method, target, body);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

fn with_server(
    tag: &str,
    setup: impl FnOnce(&Path),
    f: impl FnOnce(&Path, &Path, daemon::ServerEntry, u16),
) {
    let root = tmp(tag);
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    setup(&folder);
    let reg = root.join("registry.json");
    let dir = daemon::canonical_dir(&folder).unwrap();
    let opts = ServeOptions {
        folder: folder.clone(),
        port: 0,
        registry: reg.clone(),
        composition: None,
    };
    let handle = std::thread::spawn(move || run_serve(&opts));
    let start = Instant::now();
    let entry = loop {
        let entries = daemon::load_registry(&reg).unwrap_or_default();
        if let Some(e) = entries
            .into_iter()
            .find(|e| e.dir == dir && daemon::is_live(e.port))
        {
            break e;
        }
        if handle.is_finished() {
            panic!("server exited before serving");
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "server never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    f(&folder, &reg, entry.clone(), entry.port);
    assert!(daemon::request_shutdown(entry.port), "shutdown answers");
    assert_eq!(handle.join().expect("server thread"), 0);
    assert!(!daemon::is_live(entry.port), "port is quiet after shutdown");
}

/// The variables routes answer over HTTP: the listing, declare
/// and bind applies, preview, copies, and named refusals.
#[test]
fn variables_routes_answer_over_http() {
    with_server("http", project, |_folder, _reg, _entry, port| {
        let (code, out) = body_str(port, "GET", "/api/studio/variables", None);
        assert_eq!(code, 200, "listing: {out}");
        assert!(out.contains("\"id\":\"lonely\""), "lonely lists: {out}");
        assert!(out.contains("\"kind\":\"unused\""), "warnings: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/variables",
            Some(
                r#"{"action":"declare","actor":"maker","id":"fresh","type":"string","default":"Hi"}"#,
            ),
        );
        assert_eq!(code, 200, "declare: {out}");
        assert!(out.contains("\"saved\":true"), "receipt: {out}");
        assert!(out.contains("\"id\":\"fresh\""), "read-back: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/variables",
            Some(r#"{"action":"preview","overrides":{"title":"Flash","ghost":1}}"#),
        );
        assert_eq!(code, 200, "preview: {out}");
        assert!(out.contains("\"applied\":[\"title\"]"), "applied: {out}");
        assert!(
            out.contains("\"variable\":\"ghost\""),
            "unknown warns: {out}"
        );
        let (code, out) = body_str(
            port,
            "GET",
            "/api/studio/variables?view=values&overrides=%7B%22title%22%3A%22Flash%22%7D",
            None,
        );
        assert_eq!(code, 200, "values copy: {out}");
        assert_eq!(out, "{\"title\":\"Flash\"}", "exact values: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/variables",
            Some(
                r#"{"action":"bind","actor":"maker","target":{"id":"headline"},"property":"value","variable":"ghost"}"#,
            ),
        );
        assert_eq!(code, 409, "undeclared bind refuses: {out}");
        assert!(out.contains("--ghost"), "names it: {out}");
        let (code, out) = body_str(
            port,
            "POST",
            "/api/studio/variables",
            Some(
                r#"{"action":"bind","actor":"maker","target":{"id":"headline"},"property":"value","variable":"lonely"}"#,
            ),
        );
        assert_eq!(code, 200, "bind: {out}");
        assert!(out.contains("\"saved\":true"), "receipt: {out}");
        let (code, out) = body_str(port, "GET", "/api/studio/variables", None);
        assert_eq!(code, 200, "listing: {out}");
        assert!(!out.contains("\"id\":\"lonely\",\"type\":\"string\",\"default\":\"L\",\"label\":null,\"source\":\"disk\",\"bound\":false"), "bound flips: {out}");
    });
}
