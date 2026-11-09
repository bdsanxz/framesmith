//! Sprint 20, T002: framesmith's own skill set plus project guides.
//!
//! 11 domain skills and 10 creation workflows list with one-line
//! purposes and pass their own schema (`skills validate`); every new
//! project gains both guide variants (`AGENTS.md` and `CLAUDE.md`)
//! describing only commands this build ships; no shipped text copies
//! other skill sets or promises unbuilt capabilities.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t002-{tag}-{}-{}",
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
    run_in(args, &std::env::temp_dir())
}

fn run_in(args: &[&str], cwd: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

const DOMAINS: &[&str] = &[
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
];

const WORKFLOWS: &[&str] = &[
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
];

const CORE: &[&str] = &["router", "core", "cli", "studio"];

/// Every command verb the `framesmith` binary ships (guide references
/// must stay inside this set).
const VERBS: &[&str] = &[
    "init",
    "check",
    "lint",
    "errors",
    "catalog",
    "skills",
    "render",
    "compositions",
    "timeline",
    "info",
    "history",
    "grade-bake",
    "waveform",
    "snapshot",
    "keyframes",
    "compare",
    "grade-compare",
    "preview",
    "play",
    "preview-status",
    "preview-stop",
    "preview-list",
    "preview-stop-all",
    "preview-serve",
    "storyboard",
    "doctor",
    "benchmark",
];

fn list_json() -> serde_json::Value {
    let (code, out) = run(&["skills", "list", "--json"]);
    assert_eq!(code, 0, "{out}");
    serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"))
}

fn install_full(root: &Path, host: &str) -> PathBuf {
    let dest = root.join(format!("skills-{host}"));
    let dest_s = dest.to_str().unwrap().to_string();
    let (code, out) = run(&["skills", "install", "--host", host, "--dest", &dest_s]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&[
        "skills", "update", "--all", "--host", host, "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    dest
}

#[test]
fn list_names_all_21_skills_with_one_line_purposes() {
    let v = list_json();
    assert_eq!(v["count"], 21);
    let skills = v["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 21);
    let names: Vec<&str> = skills.iter().map(|s| s["name"].as_str().unwrap()).collect();
    for want in DOMAINS.iter().chain(WORKFLOWS.iter()) {
        assert!(names.contains(want), "list must name {want}:\n{names:?}");
    }
    let mut domains = 0;
    let mut workflows = 0;
    for s in skills {
        let purpose = s["purpose"].as_str().unwrap();
        assert!(!purpose.trim().is_empty(), "empty purpose:\n{s}");
        assert!(!purpose.contains('\n'), "purpose must be one line:\n{s}");
        match s["kind"].as_str().unwrap() {
            "domain" => domains += 1,
            "workflow" => workflows += 1,
            other => panic!("unknown kind {other}:\n{s}"),
        }
    }
    assert_eq!(domains, 11, "need 11 domain skills");
    assert_eq!(workflows, 10, "need 10 creation workflows");
    // Core set is unchanged: install still ships only four.
    for s in skills {
        let name = s["name"].as_str().unwrap();
        assert_eq!(
            s["core"].as_bool().unwrap(),
            CORE.contains(&name),
            "core membership wrong for {name}"
        );
    }
    // Human listing shows every name, the core mark, and workflow marks.
    let (code, out) = run(&["skills", "list"]);
    assert_eq!(code, 0, "{out}");
    for want in DOMAINS.iter().chain(WORKFLOWS.iter()) {
        assert!(out.contains(want), "list must name {want}:\n{out}");
    }
    for w in WORKFLOWS {
        assert!(
            out.contains(&format!("{w} —")) && out.contains("[workflow]"),
            "workflow {w} must carry the workflow mark:\n{out}"
        );
    }
}

#[test]
fn shipped_set_passes_its_own_schema() {
    let (code, out) = run(&["skills", "validate"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("every skill file passes its schema"), "{out}");
    let (code, out) = run(&["skills", "validate", "--json"]);
    assert_eq!(code, 0, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["clean"], true);
    assert_eq!(v["faults"].as_array().unwrap().len(), 0);
}

#[test]
fn installed_set_validates_and_schema_failures_name_file_and_field() {
    let root = tmp("validate-installed");
    let dest = install_full(&root, "cursor");
    let dest_s = dest.to_str().unwrap().to_string();
    // Clean install validates.
    let (code, out) = run(&["skills", "validate", "--host", "cursor", "--dest", &dest_s]);
    assert_eq!(code, 0, "{out}");
    // A corrupted document fails naming its file and field.
    let before = std::fs::read_to_string(dest.join("core/SKILL.md")).unwrap();
    let stripped: String = before
        .lines()
        .filter(|l| !l.trim_start().starts_with("Purpose:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_ne!(before, stripped);
    std::fs::write(dest.join("core/SKILL.md"), &stripped).unwrap();
    let (code, out) = run(&["skills", "validate", "--host", "cursor", "--dest", &dest_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("broken: core/SKILL.md"), "{out}");
    assert!(out.contains("field: purpose"), "{out}");
    // Same failure as JSON.
    let (code, out) = run(&[
        "skills", "validate", "--host", "cursor", "--dest", &dest_s, "--json",
    ]);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["clean"], false);
    let faults = v["faults"].as_array().unwrap();
    assert!(
        faults
            .iter()
            .any(|f| f["file"] == "core/SKILL.md" && f["field"] == "purpose"),
        "{v}"
    );
    // Restoring the shipped file clears the failure.
    let (code, out) = run(&[
        "skills", "update", "--skill", "core", "--host", "cursor", "--dest", &dest_s, "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["skills", "validate", "--host", "cursor", "--dest", &dest_s]);
    assert_eq!(code, 0, "{out}");
    // An unknown skill directory fails naming its file and field.
    std::fs::create_dir_all(dest.join("nope")).unwrap();
    std::fs::write(dest.join("nope/SKILL.md"), "# nope\n").unwrap();
    let (code, out) = run(&["skills", "validate", "--host", "cursor", "--dest", &dest_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("broken: nope/SKILL.md"), "{out}");
    assert!(out.contains("field: skill"), "{out}");
    let _ = std::fs::remove_dir_all(dest.join("nope"));
    // A missing SKILL.md fails on presence, naming the file.
    std::fs::remove_file(dest.join("studio/SKILL.md")).unwrap();
    let (code, out) = run(&["skills", "validate", "--host", "cursor", "--dest", &dest_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("broken: studio/SKILL.md"), "{out}");
    assert!(out.contains("field: presence"), "{out}");
}

#[test]
fn validate_refusals_name_host_and_flag_mistakes_exit_two() {
    let (code, out) = run(&["skills", "validate", "--host", "hal9000"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown host"), "{out}");
    assert!(out.contains("hal9000"), "{out}");
    assert!(out.contains("claude-code"), "{out}");
    // --dest without --host is a usage mistake.
    let (code, _) = run(&["skills", "validate", "--dest", "/tmp/nowhere"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["skills", "validate", "--bogus"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["skills", "bogus"]);
    assert_eq!(code, 2);
}

#[test]
fn new_projects_receive_both_guide_variants() {
    let root = tmp("guides");
    let (code, out) = run_in(&["init", "plain"], &root);
    assert_eq!(code, 0, "{out}");
    let folder = root.join("plain");
    for guide in ["AGENTS.md", "CLAUDE.md"] {
        let path = folder.join(guide);
        assert!(path.is_file(), "{guide} is missing:\n{out}");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.is_empty(), "{guide} is empty");
        assert!(body.contains("plain"), "{guide} must name the project");
        assert!(
            body.contains("no starter"),
            "{guide} must name the scaffold"
        );
        assert!(body.contains("check"), "{guide} must teach check");
        assert!(body.contains("render"), "{guide} must teach render");
    }
    let agents = std::fs::read_to_string(folder.join("AGENTS.md")).unwrap();
    let claude = std::fs::read_to_string(folder.join("CLAUDE.md")).unwrap();
    assert_ne!(agents, claude, "variants must differ by addressee");
    assert!(agents.contains("any coding agent"), "{agents}");
    assert!(claude.contains("claude-code"), "{claude}");
    // Starter projects name their starter in both variants.
    let (code, out) = run_in(&["init", "card", "--example", "title-card"], &root);
    assert_eq!(code, 0, "{out}");
    for guide in ["AGENTS.md", "CLAUDE.md"] {
        let body = std::fs::read_to_string(root.join("card").join(guide)).unwrap();
        assert!(body.contains("title-card"), "{guide} must name its starter");
    }
    // Both variants are deterministic across runs.
    let again = tmp("guides-again");
    let (code, out) = run_in(&["init", "plain"], &again);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read(root.join("plain").join("AGENTS.md")).unwrap(),
        std::fs::read(again.join("plain").join("AGENTS.md")).unwrap(),
        "guides must be deterministic"
    );
    assert_eq!(
        std::fs::read(root.join("plain").join("CLAUDE.md")).unwrap(),
        std::fs::read(again.join("plain").join("CLAUDE.md")).unwrap(),
        "guides must be deterministic"
    );
    // Guides never break the project: it still checks.
    let (code, out) = run_in(&["check", "plain"], &root);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn guides_describe_only_available_commands() {
    let root = tmp("guide-commands");
    let (code, out) = run_in(&["init", "cmds"], &root);
    assert_eq!(code, 0, "{out}");
    for guide in ["AGENTS.md", "CLAUDE.md"] {
        let body = std::fs::read_to_string(root.join("cmds").join(guide)).unwrap();
        for span in backticked(&body) {
            let mut words = span.split_whitespace();
            let first = words.next().unwrap_or("");
            if first == "framesmith" {
                let verb = words
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c| ",.:;)".contains(c));
                assert!(
                    VERBS.contains(&verb),
                    "{guide} names unavailable command `framesmith {verb}`"
                );
            }
        }
        // The exact binary must appear; phantom runners must not.
        assert!(
            body.contains("framesmith "),
            "{guide} must teach the binary"
        );
    }
}

/// Backticked spans in a Markdown body.
fn backticked(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(open) = rest.find('`') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('`') {
            out.push(rest[..close].to_string());
            rest = &rest[close + 1..];
        } else {
            break;
        }
    }
    out
}

/// Stems no shipped text may promise. A line carrying one is allowed
/// only under a denial section (`Do not:`, `Deferred:`, or the
/// guides' `Not in this build`).
const BANNED_STEMS: &[&str] = &[
    "hyperframe",
    "mcp",
    "heygen",
    "avatar",
    "voice clon",
    "voice-clon",
    "narration",
    "paid plan",
    "hosted",
    "generat",
];

fn is_header(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("## ") || t.starts_with("# ") || t.ends_with(':')
}

fn scan_clean(label: &str, body: &str) {
    let mut section = String::new();
    for line in body.lines() {
        let t = line.trim();
        // Field markers that carry their denial on the same line open
        // their own section (e.g. `Deferred: narration ...`).
        if t.starts_with("Do not:") {
            section = "Do not:".to_string();
        } else if t.starts_with("Deferred:") {
            section = "Deferred:".to_string();
        } else if is_header(line) {
            section = t.to_string();
        }
        let lower = line.to_lowercase();
        // Header lines themselves carry no promises.
        if is_header(line) {
            continue;
        }
        for stem in BANNED_STEMS {
            if lower.contains(stem) {
                assert!(
                    section == "Do not:"
                        || section == "Deferred:"
                        || section == "## Not in this build",
                    "{label} promises {stem:?} outside a denial (under {section:?}):\n{line}"
                );
            }
        }
    }
}

#[test]
fn shipped_text_is_clean_room_without_unbuilt_promises() {
    let root = tmp("cleanroom");
    let dest = install_full(&root, "claude-code");
    let v = list_json();
    let purposes: Vec<(&str, &str)> = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["name"].as_str().unwrap(), s["purpose"].as_str().unwrap()))
        .collect();
    assert_eq!(purposes.len(), 21);
    for (name, purpose) in &purposes {
        let rel = format!("{name}/SKILL.md");
        let body = std::fs::read_to_string(dest.join(&rel))
            .unwrap_or_else(|_| panic!("missing installed {rel}"));
        assert!(!body.trim().is_empty(), "{rel} is empty");
        // The listed one-line purpose is the document's own purpose.
        assert!(
            body.contains(&format!("Purpose: {purpose}")),
            "{rel} must carry its listed purpose"
        );
        scan_clean(&rel, &body);
    }
    // The router consolidates references: it names every other skill.
    let router = std::fs::read_to_string(dest.join("router/SKILL.md")).unwrap();
    for (name, _) in &purposes {
        if *name == "router" {
            continue;
        }
        assert!(
            router.contains(name),
            "router must reference every other skill, missing {name}"
        );
    }
    // Every workflow states its deferred steps with the owning sprint.
    for w in WORKFLOWS {
        let body = std::fs::read_to_string(dest.join(format!("{w}/SKILL.md"))).unwrap();
        assert!(body.contains("Deferred:"), "{w} must state deferred steps");
        assert!(body.contains("sprint"), "{w} must name the owning sprint");
        let flat = body
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            flat.contains("preview approval before"),
            "{w} must gate render on preview approval"
        );
    }
    // Both guide variants ship the same clean-room guarantees.
    let guides_root = tmp("cleanroom-guides");
    let (code, out) = run_in(&["init", "clean"], &guides_root);
    assert_eq!(code, 0, "{out}");
    for guide in ["AGENTS.md", "CLAUDE.md"] {
        let body = std::fs::read_to_string(guides_root.join("clean").join(guide)).unwrap();
        scan_clean(&format!("init {guide}"), &body);
        assert!(
            !body.to_lowercase().contains("hyperframe"),
            "{guide} must be clean-room text"
        );
    }
}
