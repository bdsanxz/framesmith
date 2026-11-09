//! Sprint 19, T003: install items in dependency order.
//!
//! Covers every acceptance criterion: exact-name installs land files
//! at manifest targets, tag installs resolve every matching item as
//! one set, dependencies write before dependents, adopted audio
//! carries its voice or sfx label, several names pass as one resolved
//! set, and cycles plus missing dependencies refuse naming the chain
//! with nothing written — plus the target validation, remapping, and
//! refusal paths. All fixture content is original clean-room work.

use catalog::{
    install, is_audio_target, project_remaps, resolve, write_plan, AdoptedAudio, InstallError,
    Selection, AUDIO_EXTENSIONS, PROJECT_CONFIG_FILE,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t003-{tag}-{}-{}",
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

fn selection(names: &[&str], tags: &[&str]) -> Selection {
    Selection {
        names: names.iter().map(|s| s.to_string()).collect(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
    }
}

const BLOCK_MANIFEST: &str = r#"{
    "name": "harbor-card",
    "type": "block",
    "title": "Harbor Card",
    "description": "An original slow card over a quiet harbor at dawn.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["titles", "harbor"]
}"#;

const BLOCK_KDL: &str = "composition name=\"harbor\" width=1280 height=720 duration=5 fps=30 background=\"#1a1a2e\" version=1 {\n    box x=80 y=200 width=400 height=120 fill=\"#f4f1de\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=5\n}\n";

const SNIPPET_KDL: &str = "// An original caption snippet.\nbox x=80 y=520 width=560 height=120 fill=\"#ffffff\" radius=12 opacity=0.96 start=0 duration=4\n";

const BASE_MANIFEST: &str = r#"{
    "name": "base-plate",
    "type": "component",
    "title": "Base Plate",
    "description": "An original plate snippet other items build on.",
    "files": [{"src": "snippet.kdl", "target": "snippets/base.kdl"}],
    "tags": ["shapes", "pack-tag"]
}"#;

const APP_MANIFEST: &str = r#"{
    "name": "harbor-app",
    "type": "block",
    "title": "Harbor App",
    "description": "An original card built on the base plate.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["titles", "pack-tag"],
    "registryDependencies": ["base-plate"]
}"#;

const APP_KDL: &str = "composition name=\"app\" width=1280 height=720 duration=5 fps=30 background=\"#101018\" version=1 {\n    box x=80 y=520 width=560 height=120 fill=\"#ffffff\" radius=12 start=0 duration=5\n    box x=80 y=520 width=16 height=120 fill=\"#e63946\" radius=6 start=0 duration=5\n}\n";

/// A three-level chain: top depends on mid, mid depends on leaf.
fn chain_registry(root: &Path) {
    for (name, dep) in [("leaf", None), ("mid", Some("leaf")), ("top", Some("mid"))] {
        let deps = match dep {
            Some(d) => format!(",\n    \"registryDependencies\": [\"{d}\"]"),
            None => String::new(),
        };
        let manifest = format!(
            "{{\n    \"name\": \"{name}\",\n    \"type\": \"component\",\n    \"title\": \"{name} piece\",\n    \"description\": \"An original chain piece.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"snippets/{name}.kdl\"}}]{deps}\n}}"
        );
        write_item(
            root,
            name,
            &manifest,
            &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
        );
    }
}

fn audio_registry(root: &Path) {
    let manifest = r#"{
        "name": "voiceover-pack",
        "type": "component",
        "title": "Voiceover Pack",
        "description": "An original narration take plus an accent sting.",
        "files": [
            {"src": "narration.wav", "target": "audio/narration.wav", "audio": "voice"},
            {"src": "sting.mp3", "target": "audio/sting.mp3", "audio": "sfx"},
            {"src": "snippet.kdl", "target": "snippets/voiceover.kdl"}
        ],
        "tags": ["voice"]
    }"#;
    write_item(
        root,
        "voiceover-pack",
        manifest,
        &[
            ("narration.wav", b"fake-wav-bytes"),
            ("sting.mp3", b"fake-mp3-bytes"),
            ("snippet.kdl", SNIPPET_KDL.as_bytes()),
        ],
    );
}

/// Exact-name installs land files at manifest targets.
#[test]
fn exact_name_install_lands_files_at_targets() {
    let root = tmp("exact");
    let registry = root.join("registry");
    write_item(
        &registry,
        "harbor",
        BLOCK_MANIFEST,
        &[("composition.kdl", BLOCK_KDL.as_bytes())],
    );
    let project = root.join("project");
    let report = install(&registry, &project, &selection(&["harbor-card"], &[])).unwrap();
    assert_eq!(report.items, vec!["harbor-card".to_string()]);
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].target, "composition.kdl");
    assert_eq!(
        std::fs::read(project.join("composition.kdl")).unwrap(),
        BLOCK_KDL.as_bytes()
    );
}

/// Dependencies write before dependents across a three-level chain.
#[test]
fn dependencies_write_before_dependents() {
    let root = tmp("chain");
    let registry = root.join("registry");
    chain_registry(&registry);
    let plan = resolve(&registry, &selection(&["top"], &[])).unwrap();
    assert_eq!(plan.order(), vec!["leaf", "mid", "top"]);
    let project = root.join("project");
    let report = write_plan(&plan, &project).unwrap();
    assert_eq!(report.items, vec!["leaf", "mid", "top"]);
    for name in ["leaf", "mid", "top"] {
        assert!(
            project.join(format!("snippets/{name}.kdl")).is_file(),
            "{name} must land"
        );
    }
}

/// Tag installs resolve every matching item as one set with shared
/// dependencies written once, before dependents.
#[test]
fn tag_install_resolves_every_matching_item_as_one_set() {
    let root = tmp("tag");
    let registry = root.join("registry");
    write_item(
        &registry,
        "base",
        BASE_MANIFEST,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "app",
        APP_MANIFEST,
        &[("composition.kdl", APP_KDL.as_bytes())],
    );
    let project = root.join("project");
    let report = install(&registry, &project, &selection(&[], &["pack-tag"])).unwrap();
    assert_eq!(report.items, vec!["base-plate", "harbor-app"]);
    assert!(
        project.join("snippets/base.kdl").is_file(),
        "base must land"
    );
    assert!(project.join("composition.kdl").is_file(), "app must land");
}

/// Several names in one call resolve as one set: shared dependencies
/// arrive once, before every dependent.
#[test]
fn several_names_pass_as_one_resolved_set() {
    let root = tmp("multi");
    let registry = root.join("registry");
    write_item(
        &registry,
        "base",
        BASE_MANIFEST,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "app",
        APP_MANIFEST,
        &[("composition.kdl", APP_KDL.as_bytes())],
    );
    let plan = resolve(&registry, &selection(&["harbor-app", "base-plate"], &[])).unwrap();
    assert_eq!(plan.order(), vec!["base-plate", "harbor-app"]);
}

/// Adopted audio carries its voice or sfx label in the plan and lands
/// beside the other files.
#[test]
fn adopted_audio_carries_voice_and_sfx_labels() {
    let root = tmp("audio");
    let registry = root.join("registry");
    audio_registry(&registry);
    let plan = resolve(&registry, &selection(&["voiceover-pack"], &[])).unwrap();
    let remaps = project_remaps(&root.join("project")).unwrap();
    assert!(remaps.is_empty());
    let adopted = catalog::adopted_audio(&plan, &remaps);
    assert_eq!(
        adopted,
        vec![
            AdoptedAudio {
                item: "voiceover-pack".to_string(),
                target: "audio/narration.wav".to_string(),
                role: catalog::AudioRole::Voice,
            },
            AdoptedAudio {
                item: "voiceover-pack".to_string(),
                target: "audio/sting.mp3".to_string(),
                role: catalog::AudioRole::Sfx,
            },
        ]
    );
    let project = root.join("project");
    let report = install(&registry, &project, &selection(&["voiceover-pack"], &[])).unwrap();
    let voice = report
        .files
        .iter()
        .find(|f| f.target == "audio/narration.wav")
        .unwrap();
    assert_eq!(voice.audio, Some(catalog::AudioRole::Voice));
    let sfx = report
        .files
        .iter()
        .find(|f| f.target == "audio/sting.mp3")
        .unwrap();
    assert_eq!(sfx.audio, Some(catalog::AudioRole::Sfx));
    assert_eq!(
        std::fs::read(project.join("audio/narration.wav")).unwrap(),
        b"fake-wav-bytes"
    );
    assert_eq!(
        std::fs::read(project.join("audio/sting.mp3")).unwrap(),
        b"fake-mp3-bytes"
    );
}

/// Audio suffix detection covers the adopted formats exactly.
#[test]
fn audio_suffix_detection_covers_adopted_formats() {
    for target in [
        "audio/take.wav",
        "audio/theme.MP3",
        "sfx/hit.ogg",
        "sfx/room.flac",
        "voice/line.m4a",
        "voice/line.aac",
        "sfx/whoosh.opus",
        "sfx/bed.wma",
        "sfx/pad.aiff",
        "sfx/pad.aif",
        "sfx/tick.oga",
    ] {
        assert!(is_audio_target(target), "{target} must read as audio");
    }
    for target in [
        "composition.kdl",
        "snippets/base.kdl",
        "docs/notes.txt",
        "audio/take",
        "audio/take.wav.bak",
    ] {
        assert!(!is_audio_target(target), "{target} must not read as audio");
    }
    assert!(!AUDIO_EXTENSIONS.is_empty());
}

/// A cycle refuses naming the chain with nothing written.
#[test]
fn cycle_refuses_naming_the_chain_with_nothing_written() {
    let root = tmp("cycle");
    let registry = root.join("registry");
    for (name, dep) in [("cyc-a", "cyc-b"), ("cyc-b", "cyc-a")] {
        let manifest = format!(
            "{{\n    \"name\": \"{name}\",\n    \"type\": \"component\",\n    \"title\": \"{name}\",\n    \"description\": \"An original cycle piece.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"snippets/{name}.kdl\"}}],\n    \"registryDependencies\": [\"{dep}\"]\n}}"
        );
        write_item(
            &registry,
            name,
            &manifest,
            &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
        );
    }
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    write_bytes(&project.join("keep.txt"), b"keep me");
    let before = tree_of(&project);
    let error = install(&registry, &project, &selection(&["cyc-a"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(
        message.contains("cyc-a") && message.contains("cyc-b") && message.contains("->"),
        "cycle must name the chain: {message}"
    );
    assert_eq!(tree_of(&project), before, "cycle must write nothing");
}

/// A missing dependency refuses naming the chain with nothing written.
#[test]
fn missing_dependency_refuses_naming_the_chain_with_nothing_written() {
    let root = tmp("missing");
    let registry = root.join("registry");
    let manifest = r#"{
        "name": "needs-ghost",
        "type": "component",
        "title": "Needs Ghost",
        "description": "An original piece with an absent dependency.",
        "files": [{"src": "snippet.kdl", "target": "snippets/needs-ghost.kdl"}],
        "registryDependencies": ["ghost-lib"]
    }"#;
    write_item(
        &registry,
        "needs-ghost",
        manifest,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&["needs-ghost"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(
        message.contains("needs-ghost") && message.contains("ghost-lib"),
        "missing dep must name the chain: {message}"
    );
    assert!(
        !project.exists() || tree_of(&project).is_empty(),
        "missing dep must write nothing"
    );
}

/// A broken dependency refuses naming the item and field.
#[test]
fn broken_dependency_refuses_naming_item_and_field() {
    let root = tmp("broken-dep");
    let registry = root.join("registry");
    write_item(
        &registry,
        "bad-child",
        r#"{"name": "bad-child", "type": "component", "title": "Bad", "description": "No files.", "files": []}"#,
        &[],
    );
    let manifest = r#"{
        "name": "broken-parent",
        "type": "component",
        "title": "Broken Parent",
        "description": "An original piece depending on a broken manifest.",
        "files": [{"src": "snippet.kdl", "target": "snippets/broken-parent.kdl"}],
        "registryDependencies": ["bad-child"]
    }"#;
    write_item(
        &registry,
        "broken-parent",
        manifest,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&["broken-parent"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(
        message.contains("bad-child") && message.contains("files"),
        "broken dep must name the item and field: {message}"
    );
    assert!(
        !project.exists() || tree_of(&project).is_empty(),
        "broken dep must write nothing"
    );
}

/// Unknown names refuse naming the item beside the available holdings.
#[test]
fn unknown_name_refuses_naming_available_with_nothing_written() {
    let root = tmp("unknown");
    let registry = root.join("registry");
    write_item(
        &registry,
        "harbor",
        BLOCK_MANIFEST,
        &[("composition.kdl", BLOCK_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&["no-such-item"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    let message = error.message().to_string();
    assert!(message.contains("no-such-item"), "{message}");
    assert!(message.contains("harbor-card"), "{message}");
    assert!(
        !project.exists() || tree_of(&project).is_empty(),
        "unknown name must write nothing"
    );
}

/// Unknown tags refuse naming the tag.
#[test]
fn unknown_tag_refuses_naming_the_tag() {
    let root = tmp("unknown-tag");
    let registry = root.join("registry");
    write_item(
        &registry,
        "harbor",
        BLOCK_MANIFEST,
        &[("composition.kdl", BLOCK_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&[], &["no-such-tag"])).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("no-such-tag"),
        "{}",
        error.message()
    );
    assert!(!project.exists() || tree_of(&project).is_empty());
}

/// An empty selection refuses before touching anything.
#[test]
fn empty_selection_refuses() {
    let root = tmp("empty");
    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&[], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(!project.exists());
}

/// Escaping, absolute, and config-clobbering targets refuse naming the
/// item and target with nothing written.
#[test]
fn bad_targets_refuse_naming_item_and_target() {
    for (tag, target) in [
        ("escape", "../escape.kdl"),
        ("absolute", "/tmp/absolute.kdl"),
        ("config", "framesmith.json"),
    ] {
        let root = tmp(&format!("bad-target-{tag}"));
        let registry = root.join("registry");
        let manifest = format!(
            "{{\n    \"name\": \"evil-{tag}\",\n    \"type\": \"component\",\n    \"title\": \"Evil {tag}\",\n    \"description\": \"An original piece with a bad target.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"{target}\"}}]\n}}"
        );
        write_item(
            &registry,
            tag,
            &manifest,
            &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
        );
        let project = root.join("project");
        let error = install(
            &registry,
            &project,
            &selection(&[&format!("evil-{tag}")], &[]),
        )
        .unwrap_err();
        assert!(error.wrote_nothing(), "{tag}");
        let message = error.message().to_string();
        assert!(
            message.contains(&format!("evil-{tag}")) && message.contains(target),
            "bad target must name item and target: {message}"
        );
        assert!(
            !project.exists() || tree_of(&project).is_empty(),
            "bad target must write nothing"
        );
    }
}

/// Two items writing one target refuse naming the target.
#[test]
fn conflicting_targets_refuse_with_nothing_written() {
    let root = tmp("conflict");
    let registry = root.join("registry");
    for name in ["first-piece", "second-piece"] {
        let manifest = format!(
            "{{\n    \"name\": \"{name}\",\n    \"type\": \"component\",\n    \"title\": \"{name}\",\n    \"description\": \"An original piece sharing a target.\",\n    \"files\": [{{\"src\": \"snippet.kdl\", \"target\": \"snippets/shared.kdl\"}}]\n}}"
        );
        write_item(
            &registry,
            name,
            &manifest,
            &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
        );
    }
    let project = root.join("project");
    let error = install(
        &registry,
        &project,
        &selection(&["first-piece", "second-piece"], &[]),
    )
    .unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("snippets/shared.kdl"),
        "{}",
        error.message()
    );
    assert!(!project.exists() || tree_of(&project).is_empty());
}

/// Target remaps from the project configuration rewrite destinations
/// before validation: longest prefix wins.
#[test]
fn remaps_rewrite_targets_through_configuration() {
    let root = tmp("remap");
    let registry = root.join("registry");
    audio_registry(&registry);
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join(PROJECT_CONFIG_FILE),
        r#"{"paths": {"audio": "assets/audio", "audio/narration.wav": "assets/voice/lead.wav"}, "registries": []}"#,
    )
    .unwrap();
    let remaps = project_remaps(&project).unwrap();
    assert_eq!(remaps.len(), 2);
    let report = install(&registry, &project, &selection(&["voiceover-pack"], &[])).unwrap();
    let targets: Vec<&str> = report.files.iter().map(|f| f.target.as_str()).collect();
    assert!(
        targets.contains(&"assets/voice/lead.wav"),
        "longest prefix must win: {targets:?}"
    );
    assert!(
        targets.contains(&"assets/audio/sting.mp3"),
        "shorter prefix still applies: {targets:?}"
    );
    assert_eq!(
        std::fs::read(project.join("assets/voice/lead.wav")).unwrap(),
        b"fake-wav-bytes"
    );
    // Unknown configuration fields stay ignored for later slices.
    assert!(project.join(PROJECT_CONFIG_FILE).is_file());
}

/// A misshapen project configuration refuses naming the file.
#[test]
fn bad_configuration_refuses_naming_the_file() {
    for (tag, contents) in [
        ("not-json", "{not json"),
        ("not-object", "[1, 2]"),
        ("bad-paths", r#"{"paths": ["audio"]}"#),
        ("bad-prefix", r#"{"paths": {"../out": "assets"}}"#),
    ] {
        let root = tmp(&format!("bad-config-{tag}"));
        let registry = root.join("registry");
        write_item(
            &registry,
            "harbor",
            BLOCK_MANIFEST,
            &[("composition.kdl", BLOCK_KDL.as_bytes())],
        );
        let project = root.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join(PROJECT_CONFIG_FILE), contents).unwrap();
        let before = tree_of(&project);
        let error: InstallError =
            install(&registry, &project, &selection(&["harbor-card"], &[])).unwrap_err();
        assert!(error.wrote_nothing(), "{tag}");
        assert!(
            error.message().contains(PROJECT_CONFIG_FILE),
            "bad config must name the file: {}",
            error.message()
        );
        assert_eq!(tree_of(&project), before, "bad config must write nothing");
    }
}

/// A name claimed by two item directories refuses rather than guessing.
#[test]
fn duplicate_names_refuse_naming_the_item() {
    let root = tmp("duplicate");
    let registry = root.join("registry");
    write_item(
        &registry,
        "first",
        BLOCK_MANIFEST,
        &[("composition.kdl", BLOCK_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "second",
        BLOCK_MANIFEST,
        &[("composition.kdl", BLOCK_KDL.as_bytes())],
    );
    let project = root.join("project");
    let error = install(&registry, &project, &selection(&["harbor-card"], &[])).unwrap_err();
    assert!(error.wrote_nothing());
    assert!(
        error.message().contains("harbor-card"),
        "{}",
        error.message()
    );
    assert!(!project.exists() || tree_of(&project).is_empty());
}

/// Resolving alone never writes: resolution runs fully before writes.
#[test]
fn resolve_alone_never_writes() {
    let root = tmp("resolve-only");
    let registry = root.join("registry");
    audio_registry(&registry);
    let project = root.join("project");
    let plan = resolve(&registry, &selection(&["voiceover-pack"], &[])).unwrap();
    assert_eq!(plan.order(), vec!["voiceover-pack"]);
    assert!(!project.exists(), "resolve must not create the project");
}

/// An unknown audio role spelling fails the manifest naming `files`.
#[test]
fn unknown_audio_role_spelling_fails_naming_files() {
    let text = r#"{
        "name": "bad-audio", "type": "component",
        "title": "Bad Audio", "description": "An original piece with a bad role.",
        "files": [{"src": "take.wav", "target": "audio/take.wav", "audio": "music"}]
    }"#;
    let errors = catalog::parse_manifest_str(text).expect_err("bad role must fail");
    assert!(
        errors.iter().any(|e| e.field == "files"),
        "must name `files`: {errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.message.contains("music")),
        "must name the spelling: {errors:?}"
    );
}

/// Fixture KDL honors the sibling hygiene gate: hyphenated attributes,
/// no external references, no fonts, no HyperFrames content.
#[test]
fn fixtures_pass_catalog_hygiene() {
    for kdl in [BLOCK_KDL, APP_KDL, SNIPPET_KDL] {
        let violations = catalog::kdl_hygiene(kdl);
        assert!(violations.is_empty(), "{violations:?} in {kdl}");
    }
    for name in [
        "harbor-card",
        "harbor-app",
        "base-plate",
        "voiceover-pack",
        "needs-ghost",
        "broken-parent",
        "bad-child",
        "cyc-a",
        "leaf",
        "mid",
        "top",
    ] {
        assert!(
            !catalog::BANNED_HYPERFRAMES_NAMES
                .iter()
                .any(|banned| banned.eq_ignore_ascii_case(name)),
            "{name} must not reuse a HyperFrames name"
        );
    }
}
