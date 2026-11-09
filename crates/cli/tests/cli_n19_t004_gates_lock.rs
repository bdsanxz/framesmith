//! Sprint 19, T004: `framesmith catalog install` gates versions and
//! locks installs.
//!
//! A gated set refuses before any write naming item, required, and
//! current versions; `--vars` bakes values into defaults or prints a
//! mount snippet; plain reinstalls keep edited files and list them;
//! force restores registry bytes; hash mismatches name the files; and
//! the lock records every hash. Installing into a valid project
//! changes no render bytes. All fixture content is original
//! clean-room work.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t004cli-{tag}-{}-{}",
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

const VARS_KDL: &str = "composition name=\"lantern-vars\" width=1280 height=720 duration=4 fps=30 background=\"#14141c\" version=1 {\nvariable id=\"headline\" type=\"string\" default=\"Hello\"\nvariable id=\"accent\" type=\"color\" default=\"#e07a5f\"\ntext value=\"var(--headline)\" start=0 duration=4 color=\"var(--accent)\"\n    box x=80 y=200 width=400 height=120 fill=\"var(--accent)\" radius=16 border-width=4 border-color=\"#e07a5f\" start=0 duration=4\n}\n";

const VARS_MANIFEST: &str = r##"{
    "name": "lantern-vars",
    "type": "block",
    "title": "Lantern Vars",
    "description": "An original card with bakable variable defaults.",
    "files": [{"src": "composition.kdl", "target": "composition.kdl"}],
    "width": 1280, "height": 720, "duration": 4,
    "tags": ["titles"],
    "variables": [
        {"id": "headline", "type": "string", "default": "Hello"},
        {"id": "accent", "type": "color", "default": "#e07a5f"}
    ]
}"##;

const SIGNAL_MANIFEST: &str = r#"{
    "name": "signal-drum",
    "type": "component",
    "title": "Signal Drum",
    "description": "An original drum hit with a tempo knob.",
    "files": [{"src": "hit.wav", "target": "audio/hit.wav"}],
    "tags": ["audio"],
    "variables": [{"id": "tempo", "type": "number", "default": 120}]
}"#;

const SNIPPET_KDL: &str = "// An original caption snippet.\nbox x=80 y=520 width=560 height=120 fill=\"#ffffff\" radius=12 opacity=0.96 start=0 duration=4\n";

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

/// Registry holding the vars card, a binary knobbed item, a gated
/// item, and the audio pack.
fn demo_registry(root: &Path) -> PathBuf {
    let registry = root.join("registry");
    write_item(
        &registry,
        "lantern-vars",
        VARS_MANIFEST,
        &[("composition.kdl", VARS_KDL.as_bytes())],
    );
    write_item(
        &registry,
        "signal-drum",
        SIGNAL_MANIFEST,
        &[("hit.wav", b"\xff\xfe\x00\x01drum-hit-bytes")],
    );
    write_item(
        &registry,
        "future-beacon",
        r#"{
            "name": "future-beacon",
            "type": "component",
            "title": "Future Beacon",
            "description": "An original piece needing a newer CLI.",
            "files": [{"src": "snippet.kdl", "target": "snippets/future-beacon.kdl"}],
            "minCliVersion": "99.0.0"
        }"#,
        &[("snippet.kdl", SNIPPET_KDL.as_bytes())],
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

/// A gated set refuses before any write, naming the item, the
/// required version, and the current version.
#[test]
fn gated_set_refuses_naming_item_and_versions_with_zero_writes() {
    let root = tmp("gate");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "future-beacon",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("future-beacon"), "{out}");
    assert!(out.contains("99.0.0"), "{out}");
    assert!(out.contains(env!("CARGO_PKG_VERSION")), "{out}");
    assert!(out.contains("nothing was written"), "{out}");
    assert!(
        !root.join("proj").exists(),
        "gated install must write nothing: {out}"
    );
}

/// `--vars` bakes values into defaults and prints a mount snippet for
/// the value with no text bake site.
#[test]
fn vars_bake_and_mount_snippet() {
    let root = tmp("vars");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "signal-drum",
            "--vars",
            "headline=Grand Opening",
            "--vars",
            "accent=#123456",
            "--vars",
            "tempo=128",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("baked vars: accent, headline"), "{out}");
    assert!(out.contains("--variables"), "{out}");
    assert!(out.contains("tempo"), "{out}");
    let text = std::fs::read_to_string(root.join("proj/composition.kdl")).unwrap();
    assert!(text.contains("default=\"Grand Opening\""), "{out}\n{text}");
    assert!(text.contains("default=\"#123456\""), "{out}\n{text}");
    assert_eq!(
        std::fs::read(root.join("proj/audio/hit.wav")).unwrap(),
        b"\xff\xfe\x00\x01drum-hit-bytes",
        "binary files land byte-identical: {out}"
    );
}

/// An unknown `--vars` id refuses naming the variable with nothing
/// written.
#[test]
fn unknown_var_refuses_naming_variable() {
    let root = tmp("unknown-var");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--vars",
            "nope=1",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("nope"), "{out}");
    assert!(out.contains("nothing was written"), "{out}");
    assert!(!root.join("proj").exists(), "{out}");
}

/// Editing an installed file then reinstalling keeps the edit and
/// names it; forcing restores registry bytes exactly.
#[test]
fn edit_reinstall_plain_then_forced() {
    let root = tmp("reinstall");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    write_bytes(
        &root.join("proj/composition.kdl"),
        format!("{VARS_KDL}// maker edit: warmer plate\n").as_bytes(),
    );
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("kept 1 edited file(s)"), "{out}");
    assert!(out.contains("composition.kdl"), "{out}");
    let kept = std::fs::read(root.join("proj/composition.kdl")).unwrap();
    assert!(
        kept.ends_with(b"// maker edit: warmer plate\n"),
        "plain reinstall must keep the edit: {out}"
    );
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--force",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("force-restored 1 file(s)"), "{out}");
    assert_eq!(
        std::fs::read(root.join("proj/composition.kdl")).unwrap(),
        VARS_KDL.as_bytes(),
        "force must restore registry bytes exactly: {out}"
    );
}

/// The lock records a hash for every installed file, each matching
/// the bytes on disk.
#[test]
fn lock_file_records_every_hash() {
    let root = tmp("lock");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "voiceover-pack",
            "--vars",
            "tempo=128",
            "--registry",
            &reg,
        ],
        &root,
    );
    // tempo is declared by signal-drum, not this set: refusal names it.
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("tempo"), "{out}");

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
    let text = std::fs::read_to_string(root.join("proj/.framesmith/catalog-lock.json")).unwrap();
    let lock: serde_json::Value = serde_json::from_str(&text).unwrap();
    let files = lock["files"].as_array().unwrap();
    assert_eq!(files.len(), 3, "{text}");
    for entry in files {
        let target = entry["target"].as_str().unwrap().to_string();
        let disk = std::fs::read(root.join("proj").join(&target)).unwrap();
        assert_eq!(
            entry["sha256"].as_str().unwrap(),
            catalog::sha256_hex(&disk),
            "lock hash for {target}"
        );
        // Cross-check against the host digest when one is available.
        let out = std::process::Command::new("shasum")
            .args(["-a", "256"])
            .arg(root.join("proj").join(&target))
            .output();
        if let Ok(out) = out {
            if out.status.success() {
                let host = String::from_utf8_lossy(&out.stdout);
                let host = host.split_whitespace().next().unwrap_or("");
                assert_eq!(
                    entry["sha256"].as_str().unwrap(),
                    host,
                    "host check for {target}"
                );
            }
        }
    }
}

/// Usage errors stay exit 2 and create nothing; duplicate `--vars`
/// ids refuse as exit 1 with nothing written.
#[test]
fn install_vars_usage_errors() {
    let root = tmp("usage");
    let registry = demo_registry(&root);
    let reg = registry_arg(&registry);
    // --vars without a value.
    let (code, _) = run(
        &["catalog", "install", "proj", "lantern-vars", "--vars"],
        &root,
    );
    assert_eq!(code, 2);
    // Duplicate --force.
    let (code, _) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--force",
            "--force",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 2);
    // Duplicate --vars id refuses with nothing written (exit 1).
    let (code, out) = run(
        &[
            "catalog",
            "install",
            "proj",
            "lantern-vars",
            "--vars",
            "headline=One",
            "--vars",
            "headline=Two",
            "--registry",
            &reg,
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("headline"), "{out}");
    assert!(out.contains("nothing was written"), "{out}");
    assert!(!root.join("proj").exists(), "{out}");
}

/// Installing into a valid project changes no render bytes: the lock
/// lands beside the composition, and the snapshot digest plus `check`
/// match the untouched project.
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
    // Install only the snippet/audio pack with a baked nothing: the
    // composition is untouched.
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
    assert!(root.join("proj/.framesmith/catalog-lock.json").is_file());
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
