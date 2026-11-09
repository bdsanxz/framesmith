//! Sprint 19, T004: version gates and the SHA256 lock.
//!
//! Covers every acceptance criterion: a gated item refuses before any
//! write naming item, required, and current versions; the lock records
//! a hash for every installed file (baked `--vars` bytes included);
//! `--vars` bakes values into `variable` defaults or prints a
//! `--variables` mount snippet; plain reinstalls keep edited files
//! and list them; force restores registry bytes exactly; and hash
//! mismatches report the edited files by name — plus the refusal
//! paths (unknown or mistyped variables, untracked collisions,
//! tool-owned targets, corrupt locks). All fixture content is
//! original clean-room work.

use catalog::{
    install_with_options, kdl_hygiene, sha256_hex, InstallOptions, Selection, VarAssign,
    BANNED_HYPERFRAMES_NAMES, CATALOG_LOCK_FILE, CURRENT_CLI_VERSION,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t004-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

/// One registry item: manifest plus file contents.
fn write_item(registry: &Path, dir: &str, manifest: &str, files: &[(&str, &[u8])]) {
    let folder = registry.join(dir);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(catalog::MANIFEST_FILE), manifest).unwrap();
    for (src, bytes) in files {
        write_bytes(&folder.join(src), bytes);
    }
}

/// Every file under a directory: relative path to bytes.
fn tree_of(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    out
}

fn selection(names: &[&str]) -> Selection {
    Selection {
        names: names.iter().map(|s| s.to_string()).collect(),
        tags: Vec::new(),
    }
}

fn options(vars: &[(&str, &str)], force: bool) -> InstallOptions {
    InstallOptions {
        current_version: CURRENT_CLI_VERSION.to_string(),
        vars: vars
            .iter()
            .map(|(id, value)| VarAssign {
                id: id.to_string(),
                value: value.to_string(),
            })
            .collect(),
        force,
    }
}

fn plain() -> InstallOptions {
    options(&[], false)
}

/// Read the lock document of an installed project.
fn lock_of(project: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(project.join(CATALOG_LOCK_FILE))
        .expect("lock file must exist after install");
    serde_json::from_str(&text).expect("lock file must be valid JSON")
}

fn lock_hashes(project: &Path) -> BTreeMap<String, String> {
    let lock = lock_of(project);
    lock["files"]
        .as_array()
        .expect("lock must carry a files array")
        .iter()
        .map(|e| {
            (
                e["target"].as_str().unwrap().to_string(),
                e["sha256"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

const VARS_KDL: &str = "composition name=\"lantern-vars\" width=1280 height=720 duration=4 fps=30 background=\"#14141c\" version=1 {\nvariable id=\"headline\" type=\"string\" default=\"Hello\"\nvariable id=\"accent\" type=\"color\" default=\"#e07a5f\"\nvariable id=\"count\" type=\"number\" default=1.5\nvariable id=\"show\" type=\"boolean\" default=true\nvariable id=\"size\" type=\"enum\" options=\"small, large\" default=\"large\"\nvariable id=\"brand\" type=\"font\" default=\"Harbor Sans, System Sans\"\ntext value=\"var(--headline)\" start=0 duration=4 x=\"var(--count)\" color=\"var(--accent)\" hidden=\"var(--show)\" align=\"var(--size)\"\n    box x=80 y=200 width=400 height=120 fill=\"var(--accent)\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=4\n}\n";

const VARS_MANIFEST: &str = r##"{
    "name": "lantern-vars",
    "type": "block",
    "title": "Lantern Vars",
    "description": "An original card with bakable variable defaults.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 4,
    "tags": ["titles", "vars-pack"],
    "variables": [
        {"id": "headline", "type": "string", "default": "Hello"},
        {"id": "accent", "type": "color", "default": "#e07a5f"},
        {"id": "count", "type": "number", "default": 1.5},
        {"id": "show", "type": "boolean", "default": false},
        {"id": "size", "type": "enum", "options": "small, large", "default": "large"},
        {"id": "brand", "type": "font", "default": "Harbor Sans, System Sans"}
    ]
}"##;

const STONE_KDL: &str = "// An original stone plate snippet.\nbox x=40 y=60 width=300 height=150 fill=\"#8a8d93\" radius=10 border-width=2 border-color=\"#3a3b40\" start=0 duration=3\n";

const STONE_MANIFEST: &str = r#"{
    "name": "stone-plate",
    "type": "component",
    "title": "Stone Plate",
    "description": "An original plate snippet for lock coverage.",
    "files": [{"src": "snippet.kdl", "target": "snippets/stone.kdl"}],
    "tags": ["shapes", "vars-pack"]
}"#;

/// A block manifest with a configurable `minCliVersion` gate.
fn gated_manifest(name: &str, min_cli: &str) -> String {
    format!(
        "{{\n    \"name\": \"{name}\",\n    \"type\": \"component\",\n    \"title\": \"{name} piece\",\n    \"description\": \"An original gated piece.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"snippets/{name}.kdl\"}}],\n    \"minCliVersion\": \"{min_cli}\"\n}}"
    )
}

/// A gated item refuses before any write, naming the item, the
/// required version, and the current version.
#[test]
fn gated_item_refuses_before_writes_naming_versions() {
    let root = tmp("gate");
    let registry = root.join("registry");
    write_item(
        &registry,
        "future-beacon",
        &gated_manifest("future-beacon", "99.0.0"),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install_with_options(
        &registry,
        &project,
        &selection(&["future-beacon"]),
        &plain(),
    )
    .unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("future-beacon"), "{message}");
    assert!(message.contains("99.0.0"), "{message}");
    assert!(message.contains(CURRENT_CLI_VERSION), "{message}");
    assert!(
        !project.exists(),
        "gated install must write nothing: {message}"
    );
}

/// A gated dependency refuses the whole set naming the dependency,
/// even when the selected root itself is ungated.
#[test]
fn gated_dependency_refuses_naming_the_chain() {
    let root = tmp("gate-dep");
    let registry = root.join("registry");
    write_item(
        &registry,
        "future-core",
        &gated_manifest("future-core", "99.0.0"),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let manifest = r#"{
        "name": "plain-frame",
        "type": "component",
        "title": "Plain Frame",
        "description": "An original ungated piece built on a gated core.",
        "files": [{"src": "snippet.kdl", "target": "snippets/plain-frame.kdl"}],
        "registryDependencies": ["future-core"]
    }"#;
    write_item(
        &registry,
        "plain-frame",
        manifest,
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    let before = tree_of(&project);
    let error = install_with_options(&registry, &project, &selection(&["plain-frame"]), &plain())
        .unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("future-core"), "{message}");
    assert!(message.contains("99.0.0"), "{message}");
    assert_eq!(tree_of(&project), before, "gated dep must write nothing");
}

/// Gates at or below the running version install: exact, lower, and
/// short (`v0.1`) spellings all pass.
#[test]
fn satisfied_gates_install() {
    let root = tmp("gate-ok");
    let registry = root.join("registry");
    write_item(
        &registry,
        "now-beacon",
        &gated_manifest("now-beacon", CURRENT_CLI_VERSION),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "old-beacon",
        &gated_manifest("old-beacon", "0.0.1"),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "short-beacon",
        &gated_manifest("short-beacon", "v0.1"),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["now-beacon", "old-beacon", "short-beacon"]),
        &plain(),
    )
    .unwrap();
    assert_eq!(report.items.len(), 3);
    assert!(report.kept.is_empty());
    for name in ["now-beacon", "old-beacon", "short-beacon"] {
        assert!(
            project.join(format!("snippets/{name}.kdl")).is_file(),
            "{name} must land"
        );
    }
}

/// An unparsable `minCliVersion` refuses fail-closed naming the item.
#[test]
fn malformed_gate_refuses_fail_closed() {
    let root = tmp("gate-bad");
    let registry = root.join("registry");
    write_item(
        &registry,
        "soon-beacon",
        &gated_manifest("soon-beacon", "soon"),
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install_with_options(&registry, &project, &selection(&["soon-beacon"]), &plain())
        .unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("soon-beacon"), "{message}");
    assert!(message.contains("minCliVersion"), "{message}");
    assert!(!project.exists());
}

/// The lock records a hash for every installed file across a
/// multi-file item, each matching the bytes on disk.
#[test]
fn lock_records_hash_for_every_installed_file() {
    let root = tmp("lock");
    let registry = root.join("registry");
    let manifest = r#"{
        "name": "lantern-pack",
        "type": "component",
        "title": "Lantern Pack",
        "description": "An original narration take, sting, and snippet.",
        "files": [
            {"src": "narration.wav", "target": "audio/narration.wav", "audio": "voice"},
            {"src": "sting.mp3", "target": "audio/sting.mp3", "audio": "sfx"},
            {"src": "snippet.kdl", "target": "snippets/lantern.kdl"}
        ],
        "tags": ["audio"]
    }"#;
    write_item(
        &registry,
        "lantern-pack",
        manifest,
        &[
            ("narration.wav", b"fake-wav-bytes"),
            ("sting.mp3", b"fake-mp3-bytes"),
            ("snippet.kdl", STONE_KDL.as_bytes()),
        ],
    );
    let project = root.join("project");
    let report =
        install_with_options(&registry, &project, &selection(&["lantern-pack"]), &plain()).unwrap();
    assert!(report.mount_snippet.is_none());
    let lock = lock_of(&project);
    let items = lock["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_str().unwrap(), "lantern-pack");
    let hashes = lock_hashes(&project);
    assert_eq!(hashes.len(), 3, "every file needs a hash: {hashes:?}");
    for target in [
        "audio/narration.wav",
        "audio/sting.mp3",
        "snippets/lantern.kdl",
    ] {
        let disk = std::fs::read(project.join(target)).unwrap();
        assert_eq!(
            hashes.get(target).map(String::as_str),
            Some(sha256_hex(&disk).as_str()),
            "lock hash must match the bytes on disk for {target}"
        );
    }
}

/// The lock covers baked bytes: with `--vars` the recorded hash
/// matches the baked file, not the registry original.
#[test]
fn lock_covers_baked_vars_bytes() {
    let root = tmp("lock-baked");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(&[("headline", "Sale"), ("accent", "#123456")], false),
    )
    .unwrap();
    assert_eq!(report.vars_baked, vec!["accent", "headline"]);
    assert!(report.mount_snippet.is_none());
    let disk = std::fs::read(project.join("composition.kdl")).unwrap();
    assert_ne!(disk, VARS_KDL.as_bytes(), "vars must bake");
    let hashes = lock_hashes(&project);
    assert_eq!(
        hashes.get("composition.kdl").map(String::as_str),
        Some(sha256_hex(&disk).as_str())
    );
    assert_ne!(
        hashes["composition.kdl"],
        sha256_hex(VARS_KDL.as_bytes()),
        "lock must cover the baked bytes, not the registry bytes"
    );
}

/// `--vars` bakes values into `variable` defaults with per-type KDL
/// spelling, and the baked document still parses.
#[test]
fn vars_bake_into_defaults() {
    let root = tmp("bake");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(
            &[
                ("headline", "Sale"),
                ("accent", "#123456"),
                ("count", "2.5"),
                ("show", "false"),
                ("size", "small"),
                ("brand", "Harbor Sans, System Sans"),
            ],
            false,
        ),
    )
    .unwrap();
    let text = std::fs::read_to_string(project.join("composition.kdl")).unwrap();
    assert!(text.contains("default=\"Sale\""), "{text}");
    assert!(text.contains("default=\"#123456\""), "{text}");
    assert!(text.contains("default=2.5"), "{text}");
    assert!(text.contains("default=false"), "{text}");
    assert!(text.contains("default=\"small\""), "{text}");
    assert!(
        text.contains("default=\"Harbor Sans, System Sans\""),
        "font try-order must survive verbatim: {text}"
    );
    assert_eq!(
        report.vars_baked,
        vec!["accent", "brand", "count", "headline", "show", "size"]
    );
    notation::parse::parse_document(&text).expect("baked KDL must still parse");
}

/// A font default bakes as one preserved try-order list: entries are
/// never split, reordered, or resolved at install time.
#[test]
fn font_try_order_preserved_verbatim() {
    let root = tmp("bake-font");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(
            &[("brand", "Zebra Face, Fallback Face, System Sans")],
            false,
        ),
    )
    .unwrap();
    let text = std::fs::read_to_string(project.join("composition.kdl")).unwrap();
    assert!(
        text.contains("variable id=\"brand\" type=\"font\" default=\"Zebra Face, Fallback Face, System Sans\""),
        "try-order list must bake verbatim: {text}"
    );
}

/// Mistyped `--vars` values refuse fail-closed with nothing written:
/// a number that is not numeric, a boolean outside true/false, and an
/// enum outside its declared options.
#[test]
fn mistyped_vars_refuse_fail_closed() {
    for (tag, id, value) in [
        ("number", "count", "lots"),
        ("boolean", "show", "maybe"),
        ("enum", "size", "huge"),
    ] {
        let root = tmp(&format!("bake-bad-{tag}"));
        let registry = root.join("registry");
        write_item(
            &registry,
            "lantern-vars",
            VARS_MANIFEST,
            &[("composition.kdl", VARS_KDL.as_bytes())],
        );
        let project = root.join("project");
        let error = install_with_options(
            &registry,
            &project,
            &selection(&["lantern-vars"]),
            &options(&[(id, value)], false),
        )
        .unwrap_err();
        assert!(error.wrote_nothing(), "{tag}");
        assert!(error.message().contains(id), "{tag}: {}", error.message());
        assert!(!project.exists(), "{tag}: mistyped vars must write nothing");
    }
}

/// A `--vars` id no manifest declares refuses naming the variable
/// instead of being silently created.
#[test]
fn unknown_var_refuses_naming_variable() {
    let root = tmp("bake-unknown");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(&[("nope", "1")], false),
    )
    .unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("nope"), "{message}");
    assert!(message.contains("headline"), "{message}");
    assert!(!project.exists());
}

/// Malformed assignments and duplicate ids refuse naming the cause.
#[test]
fn malformed_and_duplicate_assignments_refuse() {
    assert!(
        catalog::parse_var_assign("noequals").is_err(),
        "an assignment without `=` must refuse"
    );
    assert!(
        catalog::parse_var_assign("=empty").is_err(),
        "an empty id must refuse"
    );
    let ok = catalog::parse_var_assign("headline=Sale Night").unwrap();
    assert_eq!(ok.id, "headline");
    assert_eq!(ok.value, "Sale Night");
    let dupes = vec!["accent=#111111".to_string(), "accent=#222222".to_string()];
    let error = InstallOptions::with_vars(CURRENT_CLI_VERSION, &dupes, false).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(error.message().contains("accent"), "{}", error.message());
}

/// A declared value with no text bake site (binary-only item) is not
/// dropped: the report carries a `--variables` mount snippet and the
/// file lands byte-identical.
#[test]
fn unbaked_values_print_mount_snippet() {
    let root = tmp("mount");
    let registry = root.join("registry");
    let manifest = r#"{
        "name": "signal-drum",
        "type": "component",
        "title": "Signal Drum",
        "description": "An original drum hit with a tempo knob.",
        "files": [{"src": "hit.wav", "target": "audio/hit.wav"}],
        "tags": ["audio"],
        "variables": [{"id": "tempo", "type": "number", "default": 120}]
    }"#;
    let drum: &[u8] = b"\xff\xfe\x00\x01drum-hit-bytes";
    assert!(std::str::from_utf8(drum).is_err());
    write_item(&registry, "signal-drum", manifest, &[("hit.wav", drum)]);
    let project = root.join("project");
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["signal-drum"]),
        &options(&[("tempo", "128")], false),
    )
    .unwrap();
    assert!(report.vars_baked.is_empty());
    let snippet = report.mount_snippet.expect("unbaked values need a snippet");
    assert!(snippet.contains("--variables"), "{snippet}");
    assert!(snippet.contains("tempo"), "{snippet}");
    assert!(snippet.contains("128"), "{snippet}");
    assert_eq!(
        std::fs::read(project.join("audio/hit.wav")).unwrap(),
        drum,
        "binary files land byte-identical"
    );
}

/// A plain reinstall keeps locally edited files and lists them: the
/// edit survives repeated reinstalls because the lock keeps the
/// registry hash instead of laundering the edit.
#[test]
fn reinstall_plain_keeps_edits_and_lists_them() {
    let root = tmp("keep");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "stone-plate",
        STONE_MANIFEST,
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars", "stone-plate"]),
        &plain(),
    )
    .unwrap();
    let edited = format!("{VARS_KDL}// maker edit: warmer plate\n");
    write_bytes(&project.join("composition.kdl"), edited.as_bytes());
    for _ in 0..2 {
        let report = install_with_options(
            &registry,
            &project,
            &selection(&["lantern-vars", "stone-plate"]),
            &plain(),
        )
        .unwrap();
        assert_eq!(report.kept, vec!["composition.kdl".to_string()]);
        assert!(report.forced.is_empty());
        assert_eq!(
            std::fs::read(project.join("composition.kdl")).unwrap(),
            edited.as_bytes(),
            "the local edit must survive plain reinstalls"
        );
        assert_eq!(
            std::fs::read(project.join("snippets/stone.kdl")).unwrap(),
            STONE_KDL.as_bytes(),
            "untouched files still land"
        );
    }
}

/// A forced reinstall restores registry bytes exactly, listing what
/// it overwrote.
#[test]
fn reinstall_force_restores_registry_bytes() {
    let root = tmp("force");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    install_with_options(&registry, &project, &selection(&["lantern-vars"]), &plain()).unwrap();
    write_bytes(&project.join("composition.kdl"), b"maker-scratch-bytes");
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(&[], true),
    )
    .unwrap();
    assert_eq!(report.forced, vec!["composition.kdl".to_string()]);
    assert!(report.kept.is_empty());
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        VARS_KDL.as_bytes(),
        "force must restore registry bytes exactly"
    );
}

/// Hash mismatches report every edited file by name.
#[test]
fn hash_mismatch_reports_edited_files_by_name() {
    let root = tmp("mismatch");
    let registry = root.join("registry");
    let manifest = r#"{
        "name": "twin-plate",
        "type": "component",
        "title": "Twin Plate",
        "description": "An original two-file piece for mismatch naming.",
        "files": [
            {"src": "first.kdl", "target": "snippets/first.kdl"},
            {"src": "second.kdl", "target": "snippets/second.kdl"}
        ],
        "tags": ["shapes"]
    }"#;
    write_item(
        &registry,
        "twin-plate",
        manifest,
        &[
            ("first.kdl", STONE_KDL.as_bytes()),
            ("second.kdl", STONE_KDL.as_bytes()),
        ],
    );
    let project = root.join("project");
    install_with_options(&registry, &project, &selection(&["twin-plate"]), &plain()).unwrap();
    write_bytes(&project.join("snippets/first.kdl"), b"edited-first");
    write_bytes(&project.join("snippets/second.kdl"), b"edited-second");
    let report =
        install_with_options(&registry, &project, &selection(&["twin-plate"]), &plain()).unwrap();
    assert_eq!(
        report.kept,
        vec![
            "snippets/first.kdl".to_string(),
            "snippets/second.kdl".to_string()
        ]
    );
    let hashes = lock_hashes(&project);
    assert_ne!(
        sha256_hex(&std::fs::read(project.join("snippets/first.kdl")).unwrap()),
        hashes["snippets/first.kdl"]
    );
}

/// A file already sitting at a target with untracked content refuses
/// fail-closed instead of being silently overwritten or skipped;
/// `--force` states the intent explicitly.
#[test]
fn untracked_existing_file_refuses_fail_closed() {
    let root = tmp("untracked");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_bytes(
        &project.join("composition.kdl"),
        b"a maker's own composition",
    );
    let error = install_with_options(&registry, &project, &selection(&["lantern-vars"]), &plain())
        .unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("lantern-vars"), "{message}");
    assert!(message.contains("composition.kdl"), "{message}");
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        b"a maker's own composition"
    );
    let report = install_with_options(
        &registry,
        &project,
        &selection(&["lantern-vars"]),
        &options(&[], true),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        VARS_KDL.as_bytes()
    );
    assert!(report.kept.is_empty());
}

/// Install targets may never point at the lock file or anywhere else
/// inside the tool-owned hidden directory.
#[test]
fn tool_owned_targets_refuse() {
    for (tag, target) in [
        ("lock", ".framesmith/catalog-lock.json"),
        ("hidden", ".framesmith/notes.txt"),
    ] {
        let root = tmp(&format!("tool-owned-{tag}"));
        let registry = root.join("registry");
        let manifest = format!(
            "{{\n    \"name\": \"tool-{tag}\",\n    \"type\": \"component\",\n    \"title\": \"Tool {tag}\",\n    \"description\": \"An original piece with a tool-owned target.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"{target}\"}}]\n}}"
        );
        write_item(
            &registry,
            tag,
            &manifest,
            &[("snippet.kdl", STONE_KDL.as_bytes())],
        );
        let project = root.join("project");
        let error = install_with_options(
            &registry,
            &project,
            &selection(&[&format!("tool-{tag}")]),
            &plain(),
        )
        .unwrap_err();
        assert!(error.wrote_nothing(), "{tag}");
        assert!(
            error.message().contains(target),
            "{tag}: {}",
            error.message()
        );
        assert!(!project.exists() || tree_of(&project).is_empty());
    }
}

/// A corrupt lock refuses naming the lock file instead of guessing.
#[test]
fn corrupt_lock_refuses_naming_file() {
    let root = tmp("corrupt-lock");
    let registry = root.join("registry");
    write_item(
        &registry,
        "stone-plate",
        STONE_MANIFEST,
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    install_with_options(&registry, &project, &selection(&["stone-plate"]), &plain()).unwrap();
    write_bytes(&project.join(CATALOG_LOCK_FILE), b"{not json");
    let before = tree_of(&project);
    let error = install_with_options(&registry, &project, &selection(&["stone-plate"]), &plain())
        .unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains(CATALOG_LOCK_FILE),
        "{}",
        error.message()
    );
    assert_eq!(tree_of(&project), before, "corrupt lock must write nothing");
}

/// The lock merges across installs: later installs keep earlier
/// entries, and reinstalling one item never touches a sibling item's
/// edited files.
#[test]
fn lock_merges_across_installs() {
    let root = tmp("merge");
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "stone-plate",
        STONE_MANIFEST,
        &[("snippet.kdl", STONE_KDL.as_bytes())],
    );
    let project = root.join("project");
    install_with_options(&registry, &project, &selection(&["stone-plate"]), &plain()).unwrap();
    install_with_options(&registry, &project, &selection(&["lantern-vars"]), &plain()).unwrap();
    let hashes = lock_hashes(&project);
    assert_eq!(hashes.len(), 2, "both installs must be locked: {hashes:?}");
    assert!(hashes.contains_key("composition.kdl"));
    assert!(hashes.contains_key("snippets/stone.kdl"));
    let items = lock_of(&project)["items"].clone();
    let names: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["lantern-vars", "stone-plate"]);

    write_bytes(&project.join("snippets/stone.kdl"), b"maker-tuned-plate");
    let report =
        install_with_options(&registry, &project, &selection(&["lantern-vars"]), &plain()).unwrap();
    assert!(
        report.kept.is_empty(),
        "stone is not in this set: {report:?}"
    );
    assert_eq!(
        std::fs::read(project.join("snippets/stone.kdl")).unwrap(),
        b"maker-tuned-plate",
        "reinstalling a sibling item must not touch edited files"
    );
    assert!(lock_hashes(&project).contains_key("snippets/stone.kdl"));
}

/// Fixture KDL honors the sibling hygiene gate: hyphenated
/// attributes, no external references, no fonts, no HyperFrames
/// content.
#[test]
fn fixtures_pass_catalog_hygiene() {
    for kdl in [VARS_KDL, STONE_KDL] {
        let violations = kdl_hygiene(kdl);
        assert!(violations.is_empty(), "{violations:?} in {kdl}");
    }
    for name in [
        "lantern-vars",
        "lantern-pack",
        "stone-plate",
        "twin-plate",
        "signal-drum",
        "future-beacon",
        "future-core",
        "plain-frame",
        "now-beacon",
        "old-beacon",
        "short-beacon",
        "soon-beacon",
    ] {
        assert!(
            !BANNED_HYPERFRAMES_NAMES
                .iter()
                .any(|banned| banned.eq_ignore_ascii_case(name)),
            "{name} must not reuse a HyperFrames name"
        );
    }
}
