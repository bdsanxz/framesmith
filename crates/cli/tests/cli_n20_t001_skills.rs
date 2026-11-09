//! Sprint 20, T001: per-host skill install, update, and check.
//!
//! The installer ships framesmith's own clean-room skill files into
//! each named host's skill location; `check` compares the hashed
//! manifest and names drifted, missing, and extra files (reporting
//! zero drift explicitly on a clean install); `update` restores in
//! scope (all, core set, or one named skill) only with the maker's
//! say-so; unknown hosts refuse listing supported hosts and
//! unwritable locations refuse naming the path.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t001-{tag}-{}-{}",
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

fn run(args: &[&str]) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn dest_for(root: &Path, host: &str) -> PathBuf {
    root.join(format!("skills-{host}"))
}

fn install(root: &Path, host: &str) -> (i32, String) {
    let dest = dest_for(root, host);
    run(&[
        "skills",
        "install",
        "--host",
        host,
        "--dest",
        dest.to_str().unwrap(),
    ])
}

fn check(root: &Path, host: &str) -> (i32, String) {
    let dest = dest_for(root, host);
    run(&[
        "skills",
        "check",
        "--host",
        host,
        "--dest",
        dest.to_str().unwrap(),
    ])
}

const HOSTS: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "gemini",
    "copilot",
    "opencode",
];

const CORE_FILES: &[&str] = &[
    "router/SKILL.md",
    "core/SKILL.md",
    "cli/SKILL.md",
    "studio/SKILL.md",
];

#[test]
fn install_lands_core_set_per_named_host() {
    for host in HOSTS {
        let root = tmp(&format!("install-{host}"));
        let (code, out) = install(&root, host);
        assert_eq!(code, 0, "host {host}: {out}");
        let dest = dest_for(&root, host);
        for rel in CORE_FILES {
            let bytes = std::fs::read(dest.join(rel))
                .unwrap_or_else(|_| panic!("host {host}: missing {rel}:\n{out}"));
            assert!(
                !bytes.is_empty(),
                "host {host}: installed file {rel} is empty"
            );
        }
        // Core-only: extended skills stay uninstalled until T002 ships
        // them and `update --all` pulls them in.
        assert!(
            !dest.join("animation/SKILL.md").exists(),
            "host {host}: install must carry the core set only"
        );
    }
}

#[test]
fn check_clean_reports_zero_drift_explicitly() {
    let root = tmp("clean");
    let (code, _) = install(&root, "claude-code");
    assert_eq!(code, 0);
    let (code, out) = check(&root, "claude-code");
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("zero drift"),
        "clean check must report zero drift explicitly:\n{out}"
    );
}

#[test]
fn check_names_drifted_missing_and_extra() {
    let root = tmp("drift");
    let dest = dest_for(&root, "codex");
    let (code, _) = install(&root, "codex");
    assert_eq!(code, 0);
    // Drift one file, delete another, add an extra.
    let drifted = dest.join("core/SKILL.md");
    std::fs::write(&drifted, "# locally edited by the maker\n").unwrap();
    std::fs::remove_file(dest.join("cli/SKILL.md")).unwrap();
    std::fs::write(dest.join("router/notes.md"), "maker scratch").unwrap();
    let (code, out) = check(&root, "codex");
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("drifted: core/SKILL.md"), "{out}");
    assert!(out.contains("missing: cli/SKILL.md"), "{out}");
    assert!(out.contains("extra: router/notes.md"), "{out}");
}

#[test]
fn check_json_names_same_findings() {
    let root = tmp("json");
    let dest = dest_for(&root, "cursor");
    let (code, _) = install(&root, "cursor");
    assert_eq!(code, 0);
    std::fs::write(dest.join("studio/SKILL.md"), "edited").unwrap();
    let dest_s = dest.to_str().unwrap().to_string();
    let (code, out) = run(&[
        "skills", "check", "--host", "cursor", "--dest", &dest_s, "--json",
    ]);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["clean"], false);
    assert!(v["drifted"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "studio/SKILL.md"));
}

#[test]
fn update_per_name_needs_say_so_then_touches_only_its_file() {
    let root = tmp("pername");
    let dest = dest_for(&root, "gemini");
    let (code, _) = install(&root, "gemini");
    assert_eq!(code, 0);
    std::fs::write(dest.join("core/SKILL.md"), "edited").unwrap();
    std::fs::write(dest.join("router/SKILL.md"), "edited").unwrap();
    let dest_s = dest.to_str().unwrap().to_string();
    // Without --yes: refuse naming the drifted file, change nothing.
    let (code, out) = run(&[
        "skills", "update", "--skill", "core", "--host", "gemini", "--dest", &dest_s,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("drifted: core/SKILL.md"), "{out}");
    assert!(out.contains("--yes"), "{out}");
    assert_eq!(
        std::fs::read(dest.join("core/SKILL.md")).unwrap(),
        b"edited",
        "refused update must change nothing"
    );
    // With --yes: restore only the named skill.
    let (code, out) = run(&[
        "skills", "update", "--skill", "core", "--host", "gemini", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(dest.join("router/SKILL.md")).unwrap(),
        b"edited",
        "per-name update must touch only its set"
    );
    let (code, out) = run(&["skills", "check", "--host", "gemini", "--dest", &dest_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("drifted: router/SKILL.md"), "{out}");
    assert!(!out.contains("drifted: core/SKILL.md"), "{out}");
}

#[test]
fn update_core_touches_only_core_and_all_covers_extended() {
    let root = tmp("scopes");
    let dest = dest_for(&root, "copilot");
    let (code, _) = install(&root, "copilot");
    assert_eq!(code, 0);
    let dest_s = dest.to_str().unwrap().to_string();
    // Bring in the extended set, then drift one core and one extended file.
    let (code, out) = run(&[
        "skills", "update", "--all", "--host", "copilot", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(dest.join("animation/SKILL.md").exists(), "{out}");
    std::fs::write(dest.join("cli/SKILL.md"), "edited").unwrap();
    std::fs::write(dest.join("animation/SKILL.md"), "edited").unwrap();
    // --core restores the core file and leaves the extended drift alone.
    let (code, out) = run(&[
        "skills", "update", "--core", "--host", "copilot", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_ne!(
        std::fs::read(dest.join("animation/SKILL.md")).unwrap(),
        std::fs::read(dest.join("cli/SKILL.md")).unwrap_or_default(),
    );
    assert_eq!(
        std::fs::read(dest.join("animation/SKILL.md")).unwrap(),
        b"edited",
        "core update must not touch extended files:\n{out}"
    );
    // --all restores everything, and the core check is clean again.
    let (code, out) = run(&[
        "skills", "update", "--all", "--host", "copilot", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = check(&root, "copilot");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("zero drift"), "{out}");
    // An in-sync update reports sync and changes nothing, even
    // without --yes.
    let (code, out) = run(&[
        "skills", "update", "--all", "--host", "copilot", "--dest", &dest_s,
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already in sync"), "{out}");
}

#[test]
fn update_restores_missing_files() {
    let root = tmp("missing");
    let dest = dest_for(&root, "opencode");
    let (code, _) = install(&root, "opencode");
    assert_eq!(code, 0);
    std::fs::remove_file(dest.join("studio/SKILL.md")).unwrap();
    let dest_s = dest.to_str().unwrap().to_string();
    let (code, out) = run(&[
        "skills", "update", "--skill", "studio", "--host", "opencode", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(dest.join("studio/SKILL.md").exists());
    let (code, out) = check(&root, "opencode");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("zero drift"), "{out}");
}

#[test]
fn unknown_host_refuses_listing_supported_hosts() {
    let root = tmp("unknown");
    for verb in [
        vec!["skills", "install", "--host", "hal9000"],
        vec!["skills", "check", "--host", "hal9000"],
        vec!["skills", "update", "--core", "--host", "hal9000", "--yes"],
    ] {
        let (code, out) = run(&verb);
        assert_eq!(code, 1, "{verb:?}:\n{out}");
        assert!(out.contains("unknown host"), "{out}");
        assert!(out.contains("hal9000"), "{out}");
        for host in HOSTS {
            assert!(out.contains(host), "must list {host}:\n{out}");
        }
        let _ = root.clone();
    }
}

#[test]
fn unknown_skill_refuses_listing_available_skills() {
    let root = tmp("badskill");
    let dest_s = dest_for(&root, "claude-code").to_str().unwrap().to_string();
    let (code, out) = run(&[
        "skills",
        "update",
        "--skill",
        "nope",
        "--host",
        "claude-code",
        "--dest",
        &dest_s,
        "--yes",
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown skill"), "{out}");
    assert!(out.contains("\"nope\""), "{out}");
    assert!(out.contains("router"), "{out}");
}

#[test]
fn unwritable_location_refuses_naming_path() {
    let root = tmp("nowrite");
    // A regular file where the skill directory should go: every
    // create and write fails naming that path.
    let blocker = root.join("blocker");
    std::fs::write(&blocker, "in the way").unwrap();
    let blocker_s = blocker.to_str().unwrap().to_string();
    let (code, out) = run(&[
        "skills", "install", "--host", "cursor", "--dest", &blocker_s,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains(&blocker_s), "{out}");
    assert!(out.contains("writable"), "{out}");
}

#[test]
fn skills_list_names_every_skill_with_purpose() {
    let (code, out) = run(&["skills", "list"]);
    assert_eq!(code, 0, "{out}");
    // T002 grew the set to the full 21: 11 domain skills plus the 10
    // creation workflows. Install still ships the core set only.
    for skill in [
        "router",
        "core",
        "animation",
        "keyframes",
        "creative",
        "audio",
        "cli",
        "registry",
        "studio",
        "media",
        "design-import",
        "product-launch",
        "faceless-explainer",
        "pr-to-video",
        "embedded-captions",
        "talking-head-recut",
        "motion-graphics",
        "music-to-video",
        "slideshow",
        "general-video",
        "migration",
    ] {
        assert!(out.contains(skill), "list must name {skill}:\n{out}");
    }
    assert!(out.contains("(core)"), "{out}");
    let (code, out) = run(&["skills", "list", "--json"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["count"], 21);
}

#[test]
fn usage_mistakes_exit_two() {
    for args in [
        vec!["skills"],
        vec!["skills", "bogus"],
        vec!["skills", "install"],
        vec!["skills", "install", "--host", "cursor", "--bogus"],
        vec!["skills", "update", "--host", "cursor"],
        vec!["skills", "update", "--all", "--core", "--host", "cursor"],
    ] {
        let (code, out) = run(&args);
        assert_eq!(code, 2, "{args:?}:\n{out}");
    }
}
