//! Sprint 19, T003: `framesmith catalog install` installs items in
//! dependency order.
//!
//! Exact names land files at manifest targets, tags and several names
//! resolve as one set with dependents after dependencies, adopted
//! audio echoes its voice or sfx label, and cycles, missing
//! dependencies, unknown names, and bad targets refuse naming the
//! chain with byte-identical projects. Installing into a valid
//! project changes no render bytes. All fixture content is original
//! clean-room work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t003cli-{tag}-{}-{}",
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

const SNIPPET_KDL: &str = "// An original caption snippet.\nbox x=80 y=520 width=560 height=120 fill=\"#ffffff\" radius=12 opacity=0.96 start=0 duration=4\n";

const BASE_MANIFEST: &str = r#"{
    "name": "base-plate",
    "type": "component",
    "title": "Base Plate",
    "description": "An original plate snippet other items build on.",
    "files": [{"src": "snippet.kdl", "target": "snippets/base.kdl"}],
    "tags": ["shapes", "bundle-tag"]
}"#;

const APP_MANIFEST: &str = r#"{
    "name": "harbor-app",
    "type": "block",
    "title": "Harbor App",
    "description": "An original card built on the base plate.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 5,
    "tags": ["titles", "bundle-tag"],
    "registryDependencies": ["base-plate"]
}"#;

const APP_KDL: &str = "composition name=\"app\" width=640 height=360 duration=2 fps=30 background=\"#0b0b10\" version=1 {\n    box x=40 y=40 width=260 height=130 fill=\"#e63946\" radius=14 start=0 duration=2\n}\n";

const AUDIO_MANIFEST: &str = r#"{
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

/// Registry with a dependency pair plus an audio pack.
fn demo_registry(root: &Path) -> PathBuf {
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
    write_item(
        &registry,
        "voiceover-pack",
        AUDIO_MANIFEST,
        &[
            ("narration.wav", b"fake-wav-bytes"),
            ("sting.mp3", b"fake-mp3-bytes"),
            ("snippet.kdl", SNIPPET_KDL.as_bytes()),
        ],
    );
    registry
}

fn registry_arg(registry: &Path) -> String {
    registry.to_string_lossy().into_owned()
}

/// Exact-name installs echo the resolved order and land every file.
#[test]
fn exact_name_install_echoes_order_and_lands_files() {
    let root = tmp("exact");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "harbor-app",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("install order: base-plate -> harbor-app"),
        "{out}"
    );
    assert!(out.contains("base-plate: snippets/base.kdl"), "{out}");
    assert!(out.contains("harbor-app: composition.kdl"), "{out}");
    assert!(out.contains("installed 2 item(s), 2 file(s)"), "{out}");
    assert_eq!(
        std::fs::read(root.join("proj/snippets/base.kdl")).unwrap(),
        SNIPPET_KDL.as_bytes(),
        "{out}"
    );
    assert_eq!(
        std::fs::read(root.join("proj/composition.kdl")).unwrap(),
        APP_KDL.as_bytes(),
        "{out}"
    );
}

/// Tag installs and multi-name installs resolve as one set.
#[test]
fn tag_and_multi_name_installs_resolve_as_one_set() {
    let root = tmp("sets");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "by-tag",
            "unused-name-ignored",
            "--tag",
            "bundle-tag",
            "--registry",
            &reg,
        ],
        &root,
    );
    // An unknown name beside a good tag still refuses: one set, one gate.
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unused-name-ignored"), "{out}");
    assert!(
        !root.join("by-tag").exists(),
        "one set refuses together: {out}"
    );

    let (code, out) = run(
        &[
            "catalog",
            "install",
            "by-tag",
            "--tag",
            "bundle-tag",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("install order: base-plate -> harbor-app"),
        "{out}"
    );

    let (code, out) = run(
        &[
            "catalog",
            "install",
            "by-names",
            "harbor-app",
            "base-plate",
            "voiceover-pack",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("install order: base-plate -> harbor-app -> voiceover-pack"),
        "{out}"
    );
    assert!(
        out.contains("audio/narration.wav as voice"),
        "voice label must echo: {out}"
    );
    assert!(out.contains("audio/sting.mp3 as sfx"), "{out}");
}

/// Cycles refuse naming the chain with the project byte-identical.
#[test]
fn cycle_refuses_naming_the_chain_with_identical_project() {
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
    let reg = registry_arg(&registry);
    let (code, out) = run(&["init", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let before = tree_of(&root.join("proj"));
    let (code, out) = run(
        &["catalog", "install", "proj", "cyc-a", "--registry", &reg],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("cyc-a") && out.contains("cyc-b") && out.contains("->"),
        "{out}"
    );
    assert!(out.contains("nothing was written"), "{out}");
    assert_eq!(
        tree_of(&root.join("proj")),
        before,
        "cycle must write nothing"
    );
}

/// Missing dependencies refuse naming the chain with nothing created.
#[test]
fn missing_dependency_refuses_naming_the_chain() {
    let root = tmp("missing");
    let registry = root.join("registry");
    write_item(
        &registry,
        "needs-ghost",
        r#"{
            "name": "needs-ghost",
            "type": "component",
            "title": "Needs Ghost",
            "description": "An original piece with an absent dependency.",
            "files": [{"src": "snippet.kdl", "target": "snippets/needs-ghost.kdl"}],
            "registryDependencies": ["ghost-lib"]
        }"#,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "needs-ghost",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("needs-ghost") && out.contains("ghost-lib"),
        "{out}"
    );
    assert!(
        !root.join("proj").exists(),
        "missing dep must write nothing: {out}"
    );
}

/// Unknown names refuse listing the available holdings.
#[test]
fn unknown_name_refuses_listing_available() {
    let root = tmp("unknown");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "no-such-item",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no-such-item"), "{out}");
    assert!(out.contains("harbor-app"), "must list available: {out}");
    assert!(!root.join("proj").exists(), "{out}");
}

/// Unknown tags refuse naming the tag.
#[test]
fn unknown_tag_refuses_naming_the_tag() {
    let root = tmp("unknown-tag");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "--tag",
            "no-such-tag",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no-such-tag"), "{out}");
    assert!(!root.join("proj").exists(), "{out}");
}

/// Bad install targets refuse naming the item and target.
#[test]
fn bad_target_refuses_naming_item_and_target() {
    let root = tmp("bad-target");
    let registry = root.join("registry");
    write_item(
        &registry,
        "evil",
        r#"{
            "name": "evil-piece",
            "type": "component",
            "title": "Evil Piece",
            "description": "An original piece with an escaping target.",
            "files": [{"src": "snippet.kdl", "target": "../escape.kdl"}]
        }"#,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
    );
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "evil-piece",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("evil-piece") && out.contains("../escape.kdl"),
        "{out}"
    );
    assert!(!root.join("proj").exists(), "{out}");
}

/// Usage errors stay exit 2 and create nothing.
#[test]
fn install_usage_errors_stay_exit_2() {
    let root = tmp("usage");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    // No project.
    let (code, _) = run(&["catalog", "install", "--registry", &reg], &root);
    assert_eq!(code, 2);
    // Project but no names or tags.
    let (code, _) = run(&["catalog", "install", "proj", "--registry", &reg], &root);
    assert_eq!(code, 2);
    // Unknown flag.
    let (code, _) = run(
        &[
            "catalog",
            "install",
            "proj",
            "harbor-app",
            "--nope",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 2);
    // Duplicate --registry.
    let (code, _) = run(
        &[
            "catalog",
            "install",
            "proj",
            "harbor-app",
            "--registry",
            &reg,
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 2);
    // Missing --tag value.
    let (code, _) = run(&["catalog", "install", "proj", "--tag"], &root);
    assert_eq!(code, 2);
    // Unknown verb still refuses as before.
    let (code, _) = run(&["catalog", "frobnicate"], &root);
    assert_eq!(code, 2);
    assert!(
        !root.join("proj").exists(),
        "usage errors must create nothing"
    );
}

/// Remaps from the project configuration rewrite install targets.
#[test]
fn remap_configuration_rewrites_targets() {
    let root = tmp("remap");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    std::fs::create_dir_all(root.join("proj")).unwrap();
    std::fs::write(
        root.join("proj/framesmith.json"),
        r#"{"paths": {"audio": "assets/audio"}}"#,
    )
    .unwrap();
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "voiceover-pack",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("assets/audio/narration.wav as voice"), "{out}");
    assert_eq!(
        std::fs::read(root.join("proj/assets/audio/narration.wav")).unwrap(),
        b"fake-wav-bytes",
        "{out}"
    );
}

/// Installing into a valid project changes no render bytes: the
/// snippet and audio land beside the composition, and the snapshot
/// digest plus `check` match the untouched project.
#[test]
fn install_keeps_default_renders_byte_identical() {
    let root = tmp("parity");
    let (code, out) = run(&["init", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let snap_dir = root.join("snaps");
    std::fs::create_dir_all(&snap_dir).unwrap();
    let snap_arg = snap_dir.to_string_lossy().into_owned();
    let (code, out) = run(
        &["snapshot", "--at", "0", "--out", &snap_arg, "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let before = std::fs::read(snap_dir.join("snap-0s.png")).unwrap();

    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    // Install only the snippet/audio pack: the composition is untouched.
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "voiceover-pack",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    let snap_dir2 = root.join("snaps2");
    std::fs::create_dir_all(&snap_dir2).unwrap();
    let snap_arg2 = snap_dir2.to_string_lossy().into_owned();
    let (code, out) = run(
        &["snapshot", "--at", "0", "--out", &snap_arg2, "proj"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let after = std::fs::read(snap_dir2.join("snap-0s.png")).unwrap();
    assert_eq!(before, after, "install must not change render bytes");
}

/// The bundled registry installs by exact name: file bytes match the
/// bundled item contents.
#[test]
fn bundled_registry_installs_by_exact_name() {
    let root = tmp("bundled");
    let registry = root.join("bundled-registry");
    std::fs::create_dir_all(&registry).unwrap();
    let count = catalog::materialize_bundled(&registry).unwrap();
    assert!(count >= 1, "bundled registry must hold items");
    let bundled = catalog::BUNDLED
        .iter()
        .find(|b| b.item_type == catalog::ItemType::Component)
        .expect("bundled registry must hold a component");
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            bundled.name,
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains(bundled.name), "{out}");
    for file in bundled.files {
        let expected = file.contents.replace("{name}", bundled.name);
        let actual = std::fs::read(root.join("proj").join(file.target)).unwrap();
        assert_eq!(actual, expected.as_bytes(), "target {}", file.target);
    }
}
