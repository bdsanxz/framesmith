//! Sprint 19, T007: custom registries end to end.
//!
//! A project configuration names extra registries merged into search
//! and install with the registry of origin shown: custom items are
//! found and installed like bundled ones, collisions follow the
//! configured order both ways, an unreachable registry warns by name
//! while bundled keeps working, and an invalid custom manifest is
//! skipped naming its file. Every fixture is our own clean-room
//! content with hyphenated KDL style attributes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t007-cli-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_framesmith"))
}

fn run(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

fn write_item(registry: &Path, dir: &str, manifest: &str, files: &[(&str, &[u8])]) {
    let folder = registry.join(dir);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("manifest.json"), manifest).unwrap();
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

/// Numbered picker picks, ignoring `warning:` lines on stderr.
fn picks(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| !l.starts_with("warning:"))
        .filter_map(|l| l.split_once(". "))
        .map(|(_, rest)| rest.split_whitespace().next().unwrap().to_string())
        .collect()
}

const BUNDLED_MANIFEST: &str = r#"{
    "name": "meadow-card",
    "type": "block",
    "title": "Meadow Card",
    "description": "An original slow card over a quiet meadow at dusk.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["meadow"]
}"#;

const BUNDLED_KDL: &str = "composition name=\"meadow\" width=1280 height=720 duration=5 fps=30 background=\"#1a1a2e\" version=1 {\n    box x=80 y=200 width=400 height=120 fill=\"#f4f1de\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=5\n}\n";

const CUSTOM_MANIFEST: &str = r#"{
    "name": "team-banner",
    "type": "component",
    "title": "Team Banner",
    "description": "An original shared banner snippet from the team registry.",
    "files": [{"src": "snippet.kdl", "target": "snippets/team-banner.kdl"}],
    "tags": ["team"]
}"#;

const CUSTOM_KDL: &str = "// An original team banner snippet.\nbox x=80 y=120 width=560 height=96 fill=\"#fefae0\" radius=12 border-width=2 border-color=\"#606c38\" start=0 duration=4\n";

/// Base registry plus team registry fixtures; returns the `--registry`
/// flag value for the base.
fn fixtures(root: &Path) -> String {
    let base = root.join("registry");
    write_item(
        &base,
        "meadow-card",
        BUNDLED_MANIFEST,
        &[("composition.kdl", BUNDLED_KDL.as_bytes())],
    );
    let team = root.join("team-registry");
    write_item(
        &team,
        "team-banner",
        CUSTOM_MANIFEST,
        &[("snippet.kdl", CUSTOM_KDL.as_bytes())],
    );
    base.to_string_lossy().into_owned()
}

/// Search finds the custom-only item with its registry of origin shown.
#[test]
fn search_finds_custom_items_with_origin_shown() {
    let root = tmp("search");
    let reg = fixtures(&root);
    std::fs::write(
        root.join("framesmith.json"),
        r#"{"registries": [{"name": "team", "path": "team-registry"}]}"#,
    )
    .unwrap();
    let (code, out) = run(&["catalog", "search", "banner", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(picks(&out), vec!["team-banner"], "{out}");
    assert!(out.contains("[team]"), "origin is shown: {out}");
    assert!(out.contains("framesmith catalog install"), "{out}");
    // The bundled item still ranks with its own origin.
    let (code, out) = run(&["catalog", "search", "meadow", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(picks(&out), vec!["meadow-card"], "{out}");
    assert!(out.contains("[bundled]"), "bundled origin is shown: {out}");
}

/// `show` names the custom origin; `--json` carries it plus warnings.
#[test]
fn show_names_the_custom_origin() {
    let root = tmp("show");
    let reg = fixtures(&root);
    std::fs::write(
        root.join("framesmith.json"),
        r#"{"registries": [{"name": "team", "path": "team-registry"}]}"#,
    )
    .unwrap();
    let (code, out) = run(
        &["catalog", "show", "team-banner", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("team-banner"), "{out}");
    assert!(out.contains("\"team\""), "origin is named: {out}");
    let (code, out) = run(
        &[
            "catalog",
            "show",
            "team-banner",
            "--json",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["origin"], "team");
    assert_eq!(value["found"], true);
}

/// A custom-registry-only item installs like a bundled one: bytes
/// land at manifest targets and the echo names the origin.
#[test]
fn install_installs_custom_only_items() {
    let root = tmp("install");
    let reg = fixtures(&root);
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("framesmith.json"),
        r#"{"registries": [{"name": "team", "path": "../team-registry"}]}"#,
    )
    .unwrap();
    let proj = project.to_string_lossy().into_owned();
    let (code, out) = run(
        &[
            "catalog",
            "install",
            &proj,
            "team-banner",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("install order: team-banner"), "{out}");
    assert!(out.contains("team-banner [team]"), "{out}");
    assert_eq!(
        std::fs::read(project.join("snippets/team-banner.kdl")).unwrap(),
        CUSTOM_KDL.as_bytes()
    );
}

/// Collisions follow the configured order: bundled wins by default,
/// the `beforeBundled` custom registry wins when configured.
#[test]
fn install_collisions_follow_the_configured_order() {
    for (tag, entries, winner_kdl) in [
        (
            "bundled-wins",
            r#"{"registries": [{"name": "team", "path": "../team-registry"}]}"#,
            BUNDLED_KDL,
        ),
        (
            "custom-wins",
            r#"{"registries": [{"name": "team", "path": "../team-registry", "beforeBundled": true}]}"#,
            CUSTOM_KDL,
        ),
    ] {
        let root = tmp(tag);
        let base = root.join("registry");
        write_item(
            &base,
            "meadow-card",
            BUNDLED_MANIFEST,
            &[("composition.kdl", BUNDLED_KDL.as_bytes())],
        );
        let team = root.join("team-registry");
        let manifest = BUNDLED_MANIFEST.replace(
            "An original slow card over a quiet meadow at dusk.",
            "A team variant of the meadow card.",
        );
        write_item(
            &team,
            "meadow-card",
            &manifest,
            &[("composition.kdl", CUSTOM_KDL.as_bytes())],
        );
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("framesmith.json"), entries).unwrap();
        let proj = project.to_string_lossy().into_owned();
        let reg = base.to_string_lossy().into_owned();
        let (code, out) = run(
            &[
                "catalog",
                "install",
                &proj,
                "meadow-card",
                "--registry",
                &reg,
            ],
            &root,
        );
        assert_eq!(code, 0, "{out}");
        assert!(
            out.contains("collision"),
            "{tag}: the collision is reported: {out}"
        );
        assert_eq!(
            std::fs::read(project.join("composition.kdl")).unwrap(),
            winner_kdl.as_bytes(),
            "{tag}: the configured winner's bytes must land"
        );
    }
}

/// An unreachable custom registry warns by name; the bundled base
/// keeps serving search and install.
#[test]
fn unreachable_custom_registries_warn_by_name() {
    let root = tmp("unreachable");
    let reg = fixtures(&root);
    std::fs::write(
        root.join("framesmith.json"),
        r#"{"registries": [{"name": "ghost", "path": "no-such-registry"}, {"name": "team", "path": "team-registry"}]}"#,
    )
    .unwrap();
    let (code, out) = run(&["catalog", "search", "meadow", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(picks(&out), vec!["meadow-card"], "{out}");
    assert!(out.contains("\"ghost\""), "warns by name: {out}");
    assert!(out.contains("unreachable"), "{out}");
    // The custom item beside the unreachable registry still installs.
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("framesmith.json"),
        r#"{"registries": [{"name": "ghost", "path": "../no-such-registry"}, {"name": "team", "path": "../team-registry"}]}"#,
    )
    .unwrap();
    let proj = project.to_string_lossy().into_owned();
    let (code, out) = run(
        &[
            "catalog",
            "install",
            &proj,
            "team-banner",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"ghost\""), "install warns by name: {out}");
    assert_eq!(
        std::fs::read(project.join("snippets/team-banner.kdl")).unwrap(),
        CUSTOM_KDL.as_bytes()
    );
}

/// An invalid custom manifest is skipped naming its file while the
/// healthy custom item still installs.
#[test]
fn invalid_custom_manifests_skip_naming_the_file() {
    let root = tmp("invalid");
    let reg = fixtures(&root);
    let team = root.join("team-registry");
    std::fs::create_dir_all(team.join("broken-piece")).unwrap();
    std::fs::write(
        team.join("broken-piece").join("manifest.json"),
        r#"{"name": "broken-piece", "type": "block", "title": "Broken",
            "description": "A block with no canvas.", "files": []}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("framesmith.json"),
        r#"{"registries": [{"name": "team", "path": "team-registry"}]}"#,
    )
    .unwrap();
    let (code, out) = run(&["catalog", "search", "banner", "--registry", &reg], &root);
    assert_eq!(code, 0, "{out}");
    assert_eq!(picks(&out), vec!["team-banner"], "{out}");
    assert!(out.contains("manifest.json"), "skip names the file: {out}");
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("framesmith.json"),
        r#"{"registries": [{"name": "team", "path": "../team-registry"}]}"#,
    )
    .unwrap();
    let proj = project.to_string_lossy().into_owned();
    let (code, out) = run(
        &[
            "catalog",
            "install",
            &proj,
            "team-banner",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(project.join("snippets/team-banner.kdl")).unwrap(),
        CUSTOM_KDL.as_bytes()
    );
}

/// Machine output carries origins and warnings and stays parseable.
#[test]
fn search_json_carries_origins_and_warnings() {
    let root = tmp("json");
    let reg = fixtures(&root);
    std::fs::write(
        root.join("framesmith.json"),
        r#"{"registries": [{"name": "ghost", "path": "no-such-registry"}, {"name": "team", "path": "team-registry"}]}"#,
    )
    .unwrap();
    let (code, out) = run(
        &["catalog", "search", "banner", "--json", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["hits"][0]["name"], "team-banner");
    assert_eq!(value["hits"][0]["origin"], "team");
    let warnings = value["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("\"ghost\"")),
        "{value}"
    );
}

/// A misshapen `registries` value refuses the install naming the
/// configuration file with nothing written.
#[test]
fn bad_registries_config_refuses_before_any_write() {
    let root = tmp("bad-config");
    let reg = fixtures(&root);
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("framesmith.json"), r#"{"registries": [42]}"#).unwrap();
    let before = tree_of(&project);
    let proj = project.to_string_lossy().into_owned();
    let (code, out) = run(
        &[
            "catalog",
            "install",
            &proj,
            "team-banner",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("framesmith.json"), "{out}");
    assert_eq!(tree_of(&project), before, "refusals write nothing");
}
