//! Sprint 20, T005: creation workflow documents as executable contracts.
//!
//! The 10 workflow documents each state input, output, and steps,
//! call only the locally available capability menu, name approval
//! gates, name the owning sprint for deferred steps, and stop by
//! name when a capability is absent. Excluded hosted or generative
//! steps are never simulated. Corrupted contracts refuse through
//! `skills validate` naming the file and the field.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t005-{tag}-{}-{}",
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

/// The local capability menu: every verb a workflow may call. This
/// mirrors `WORKFLOW_CAPABILITY_MENU` in `crates/cli/src/skills.rs`;
/// the shipped `skills validate` enforces the same set, so any drift
/// between the two fails the suite below.
const LOCAL_MENU: &[&str] = &[
    "check",
    "compare",
    "compositions",
    "grade-bake",
    "grade-compare",
    "history",
    "info",
    "keyframes",
    "render",
    "snapshot",
    "timeline",
    "waveform",
];

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

fn read_doc(dest: &Path, workflow: &str) -> String {
    std::fs::read_to_string(dest.join(format!("{workflow}/SKILL.md")))
        .unwrap_or_else(|_| panic!("missing installed {workflow}/SKILL.md"))
}

/// Text of one `Marker:` section: lines from the marker until the
/// next section marker or the end of the document.
fn section(body: &str, marker: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with(marker) {
            inside = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if inside
            && (t.starts_with("Input:")
                || t.starts_with("Steps:")
                || t.starts_with("Capabilities:")
                || t.starts_with("Gates:")
                || t.starts_with("Deferred:")
                || t.starts_with("Stop:")
                || t.starts_with("Output:")
                || t.starts_with("Do:")
                || t.starts_with("Do not:"))
            && !t.starts_with(marker)
        {
            break;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Backticked capability tokens inside a section of text.
fn backticked_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('`') else {
            break;
        };
        let span = rest[..close].trim().to_string();
        rest = &rest[close + 1..];
        let first = span.split_whitespace().next().unwrap_or("");
        let name = first.trim_matches(|c: char| ",.:;)".contains(c));
        if !name.is_empty() {
            out.push(name.to_string());
        }
    }
    out
}

#[test]
fn all_ten_contracts_state_input_output_and_steps() {
    let root = tmp("contracts");
    let dest = install_full(&root, "claude-code");
    let (code, out) = run(&["skills", "validate"]);
    assert_eq!(code, 0, "shipped set must validate:\n{out}");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        for marker in ["Input:", "Output:", "Steps:"] {
            let text = section(&body, marker);
            assert!(!text.trim().is_empty(), "{w} is missing {marker}");
            // The marker line alone is not a contract: the marker
            // line's own content plus any following lines must say
            // something.
            let mut lines = text.lines();
            let first = lines.next().unwrap_or("").replacen(marker, "", 1);
            let rest: String = std::iter::once(first.as_str())
                .chain(lines)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !rest.trim().is_empty(),
                "{w} states {marker} with no content"
            );
        }
        // Steps walk the contract in order: interview, board, build,
        // then the gated render.
        let steps = section(&body, "Steps:").to_lowercase();
        assert!(steps.contains("interview"), "{w} steps skip the interview");
        assert!(
            steps.contains("board") || steps.contains("stills"),
            "{w} steps skip the board"
        );
        assert!(steps.contains("render"), "{w} steps never reach render");
    }
}

#[test]
fn callable_capabilities_come_only_from_the_local_menu() {
    let root = tmp("menu");
    let dest = install_full(&root, "codex");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        let caps = section(&body, "Capabilities:");
        assert!(!caps.trim().is_empty(), "{w} is missing Capabilities:");
        let tokens = backticked_tokens(&caps);
        assert!(!tokens.is_empty(), "{w} names no capabilities");
        for token in &tokens {
            assert!(
                LOCAL_MENU.contains(&token.as_str()),
                "{w} calls {token:?}, outside the local menu {LOCAL_MENU:?}"
            );
        }
    }
    // Every menu member is a verb this build actually ships: the
    // binary help names each one, so no workflow can call a phantom.
    let (code, help) = run(&["--help"]);
    assert_eq!(code, 0, "{help}");
    for verb in LOCAL_MENU {
        assert!(
            help.contains(verb),
            "menu member {verb:?} is not a shipped command"
        );
    }
}

#[test]
fn every_workflow_names_its_approval_gates() {
    let root = tmp("gates");
    let dest = install_full(&root, "cursor");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        let gates = section(&body, "Gates:");
        assert!(!gates.trim().is_empty(), "{w} is missing Gates:");
        let flat = body
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // Nothing animates before the board is approved ...
        assert!(
            flat.contains("approval before"),
            "{w} never gates motion on approval"
        );
        // ... and nothing renders before the preview is approved.
        assert!(
            flat.contains("preview approval before"),
            "{w} must gate render on preview approval"
        );
    }
}

#[test]
fn deferred_steps_name_their_owning_sprint() {
    let root = tmp("deferred");
    let dest = install_full(&root, "gemini");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        let deferred = section(&body, "Deferred:");
        assert!(!deferred.trim().is_empty(), "{w} is missing Deferred:");
        assert!(
            deferred.to_lowercase().contains("sprint"),
            "{w} defers steps without naming the owning sprint"
        );
    }
}

#[test]
fn absent_capabilities_stop_the_step_by_name() {
    let root = tmp("stop");
    let dest = install_full(&root, "copilot");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        let stop = section(&body, "Stop:");
        assert!(!stop.trim().is_empty(), "{w} is missing Stop:");
        let flat = stop
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        // The stop names what is missing and who owns it ...
        assert!(
            flat.contains("naming the capability"),
            "{w} stop rule does not name the capability"
        );
        assert!(flat.contains("sprint"), "{w} stop rule names no sprint");
        // ... refuses to fake it ...
        assert!(
            flat.contains("never simulate"),
            "{w} stop rule must refuse simulation"
        );
        // ... and keeps earlier work.
        assert!(
            flat.contains("resumable") || flat.contains("stays valid"),
            "{w} stop rule must keep prior steps resumable"
        );
    }
    // Walk one workflow to its deferred step: talking-head-recut
    // cuts maker-noted gaps, then reaches automatic silence
    // detection, which stops naming the capability and its sprint
    // while the cut steps before it stay valid and resumable.
    let body = read_doc(&dest, "talking-head-recut");
    let steps = section(&body, "Steps:");
    assert!(
        steps.contains("gaps"),
        "recut steps must cut the gaps first"
    );
    let deferred = section(&body, "Deferred:");
    assert!(
        deferred.contains("automatic silence detection"),
        "recut must defer automatic silence detection"
    );
    let stop = section(&body, "Stop:");
    let flat_stop = stop.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat_stop.contains("automatic silence detection"),
        "recut stop must name the absent step"
    );
    assert!(
        flat_stop.contains("ML sprint"),
        "recut stop must name the sprint"
    );
    assert!(
        stop.contains("resumable"),
        "recut prior steps must stay resumable"
    );
}

/// Stems no shipped text may promise. A line carrying one is allowed
/// only under a denial section (`Do not:`, `Deferred:`, or the Stop
/// rule that continues a Deferred section).
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

#[test]
fn excluded_hosted_or_generative_steps_are_never_simulated() {
    let root = tmp("excluded");
    let dest = install_full(&root, "opencode");
    for w in WORKFLOWS {
        let body = read_doc(&dest, w);
        // Every contract refuses simulation in words, not just shape.
        let flat = body
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            flat.contains("never simulat"),
            "{w} must state the missing step is never simulated"
        );
        // Banned stems appear only under denial sections.
        let mut section_name = String::new();
        for line in body.lines() {
            let t = line.trim();
            if t.starts_with("Do not:") {
                section_name = "Do not:".to_string();
            } else if t.starts_with("Deferred:") {
                section_name = "Deferred:".to_string();
            } else if is_header(line) {
                section_name = t.to_string();
            }
            if is_header(line) {
                continue;
            }
            let lower = line.to_lowercase();
            for stem in BANNED_STEMS {
                if lower.contains(stem) {
                    assert!(
                        section_name == "Do not:"
                            || section_name == "Deferred:"
                            || section_name == "## Not in this build",
                        "{w} promises {stem:?} outside a denial (under {section_name:?}):\n{line}"
                    );
                }
            }
        }
    }
}

#[test]
fn validate_refuses_a_workflow_missing_its_stop() {
    let root = tmp("refuse-stop");
    let dest = install_full(&root, "claude-code");
    let dest_s = dest.to_str().unwrap().to_string();
    let rel = "slideshow/SKILL.md";
    let before = std::fs::read_to_string(dest.join(rel)).unwrap();
    assert!(before.contains("Stop:"), "fixture must carry Stop:");
    let stripped: String = before
        .lines()
        .filter(|l| !l.trim_start().starts_with("Stop:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(dest.join(rel), &stripped).unwrap();
    let (code, out) = run(&[
        "skills",
        "validate",
        "--host",
        "claude-code",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 1, "missing Stop: must refuse:\n{out}");
    assert!(out.contains("broken: slideshow/SKILL.md"), "{out}");
    assert!(out.contains("field: stop"), "{out}");
    let (code, out) = run(&[
        "skills",
        "validate",
        "--host",
        "claude-code",
        "--dest",
        &dest_s,
        "--json",
    ]);
    assert_eq!(code, 1, "{out}");
    let v: serde_json::Value =
        serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"));
    assert_eq!(v["clean"], false);
    assert!(
        v["faults"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["file"] == "slideshow/SKILL.md" && f["field"] == "stop"),
        "{v}"
    );
    // Restoring the shipped file clears the refusal.
    let (code, out) = run(&[
        "skills",
        "update",
        "--skill",
        "slideshow",
        "--host",
        "claude-code",
        "--dest",
        &dest_s,
        "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&[
        "skills",
        "validate",
        "--host",
        "claude-code",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn validate_refuses_an_off_menu_capability() {
    let root = tmp("refuse-menu");
    let dest = install_full(&root, "codex");
    let dest_s = dest.to_str().unwrap().to_string();
    let rel = "music-to-video/SKILL.md";
    let before = std::fs::read_to_string(dest.join(rel)).unwrap();
    let tampered = before.replace(
        "`waveform`, `timeline`,",
        "`waveform`, `timeline`, `heygen`,",
    );
    assert_ne!(before, tampered, "fixture edit must apply");
    std::fs::write(dest.join(rel), &tampered).unwrap();
    let (code, out) = run(&["skills", "validate", "--host", "codex", "--dest", &dest_s]);
    assert_eq!(code, 1, "off-menu capability must refuse:\n{out}");
    assert!(out.contains("broken: music-to-video/SKILL.md"), "{out}");
    assert!(out.contains("field: capabilities"), "{out}");
    assert!(out.contains("heygen"), "{out}");
    // The baked set is untouched by the tampered install.
    let (code, out) = run(&["skills", "validate"]);
    assert_eq!(code, 0, "shipped set must stay clean:\n{out}");
    // Restoring the shipped file clears the refusal.
    let (code, out) = run(&[
        "skills",
        "update",
        "--skill",
        "music-to-video",
        "--host",
        "codex",
        "--dest",
        &dest_s,
        "--yes",
    ]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["skills", "validate", "--host", "codex", "--dest", &dest_s]);
    assert_eq!(code, 0, "{out}");
}
