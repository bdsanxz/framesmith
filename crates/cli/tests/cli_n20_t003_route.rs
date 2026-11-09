//! Sprint 20, T003: the router reads the request plus project state,
//! interviews (one question per must-have), pitches five concepts,
//! names the chosen workflow with reasons, installs a creation
//! workflow only when needed, offers general plus Not-sure on
//! ambiguity, and reports a corrupt brief with a fresh offer. Routing
//! never writes into the project: renders stay byte-identical.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t003-{tag}-{}-{}",
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

fn route_json(args: &[&str]) -> serde_json::Value {
    let mut full: Vec<&str> = args.to_vec();
    full.push("--json");
    let (code, out) = run(&full);
    assert_eq!(code, 0, "{out}");
    serde_json::from_str(&out).unwrap_or_else(|_| panic!("not JSON:\n{out}"))
}

fn make_project(root: &Path, name: &str) -> PathBuf {
    let (code, out) = run_in(&["init", name], root);
    assert_eq!(code, 0, "{out}");
    root.join(name)
}

/// One request per scorable workflow that must win strictly.
const CLEAR_ROUTES: &[(&str, &str)] = &[
    (
        "a product launch film for our new headphones",
        "product-launch",
    ),
    (
        "a voice-free explainer tutorial walking through the setup",
        "faceless-explainer",
    ),
    (
        "summarize this pull request diff and changelog for reviewers",
        "pr-to-video",
    ),
    (
        "burn readable captions with subtitles for the final cut",
        "embedded-captions",
    ),
    (
        "recut my talking head interview into a tighter cut",
        "talking-head-recut",
    ),
    ("a kinetic motion graphics opener", "motion-graphics"),
    ("cut vacation clips to our favorite song", "music-to-video"),
    (
        "a photo slideshow of stills for the lobby screen",
        "slideshow",
    ),
    (
        "migrate my old foreign project into framesmith",
        "migration",
    ),
];

/// Must-have counts per workflow: the interview asks exactly one
/// question per must-have.
const MUST_HAVE_COUNTS: &[(&str, usize)] = &[
    ("product-launch", 3),
    ("faceless-explainer", 2),
    ("pr-to-video", 2),
    ("embedded-captions", 2),
    ("talking-head-recut", 2),
    ("motion-graphics", 2),
    ("music-to-video", 3),
    ("slideshow", 3),
    ("general-video", 2),
    ("migration", 3),
];

#[test]
fn clear_requests_name_the_workflow_with_reasons() {
    for (request, workflow) in CLEAR_ROUTES {
        let v = route_json(&["route", "--request", request]);
        assert_eq!(v["choice"]["workflow"], *workflow, "{request}");
        let reasons = v["choice"]["reasons"].as_array().unwrap();
        assert!(!reasons.is_empty(), "reasons must name why: {request}");
        assert!(
            reasons
                .iter()
                .any(|r| r.as_str().unwrap().contains("request mentions")),
            "reasons must cite matched words: {request}:\n{reasons:?}"
        );
        // Five concepts, every one a grounded proposal.
        let concepts = v["concepts"].as_array().unwrap();
        assert_eq!(concepts.len(), 5, "{request}");
        for c in concepts {
            assert!(!c["title"].as_str().unwrap().is_empty(), "{request}");
            assert!(!c["hook"].as_str().unwrap().is_empty(), "{request}");
            assert!(
                c["hook"].as_str().unwrap().contains("Proposal:"),
                "concepts are proposals, not facts: {request}:\n{c}"
            );
        }
    }
}

#[test]
fn clear_request_human_names_workflow_reasons_and_pitch() {
    let (code, out) = run(&[
        "route",
        "--request",
        "a product launch film for our new headphones",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("chosen workflow: product-launch"), "{out}");
    assert!(out.contains("reason:"), "{out}");
    assert!(
        out.contains("interview (3 question(s), one per must-have)"),
        "{out}"
    );
    assert!(
        out.contains("pitch (5 concept(s), proposals, not facts)"),
        "{out}"
    );
    assert!(out.contains("run shape: single"), "{out}");
    assert!(out.contains("single, batch, stills-only"), "{out}");
}

#[test]
fn interview_asks_one_question_per_must_have_for_every_workflow() {
    for (workflow, count) in MUST_HAVE_COUNTS {
        // A neutral request scores nothing; the explicit maker pick
        // still interviews that workflow's own must-haves.
        let v = route_json(&["route", "--request", "make a video", "--workflow", workflow]);
        assert_eq!(v["choice"]["workflow"], *workflow, "{workflow}");
        let interview = v["interview"].as_array().unwrap();
        assert_eq!(
            interview.len(),
            *count,
            "one question per must-have: {workflow}"
        );
        let reasons = v["choice"]["reasons"].as_array().unwrap();
        assert!(
            reasons
                .iter()
                .any(|r| r.as_str().unwrap().contains("explicitly")),
            "explicit picks say so: {workflow}:\n{reasons:?}"
        );
        // Concepts follow the chosen workflow.
        let hooks: Vec<&str> = v["concepts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["hook"].as_str().unwrap())
            .collect();
        assert!(
            hooks.iter().any(|h| h.contains(workflow)),
            "pitch follows the choice: {workflow}:\n{hooks:?}"
        );
    }
}

#[test]
fn ambiguity_offers_general_plus_not_sure_never_a_guess() {
    // No keywords at all.
    let v = route_json(&["route", "--request", "make something nice for friday"]);
    assert!(v["choice"].is_null(), "{v}");
    assert_eq!(
        v["offered"],
        serde_json::json!(["general-video", "not-sure"]),
        "{v}"
    );
    assert_eq!(v["concepts"].as_array().unwrap().len(), 5, "{v}");
    // A tie for first is ambiguous too.
    let (code, out) = run(&["route", "--request", "launch the slideshow"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("chosen workflow: none (ambiguous; not guessing)"),
        "{out}"
    );
    assert!(out.contains("offer: general-video, not-sure"), "{out}");
    assert!(
        out.contains("equally; needs a maker pick, not a guess"),
        "{out}"
    );
}

#[test]
fn workflow_override_resolves_ambiguity() {
    let v = route_json(&[
        "route",
        "--request",
        "make something nice for friday",
        "--workflow",
        "slideshow",
    ]);
    assert_eq!(v["choice"]["workflow"], "slideshow", "{v}");
    assert_eq!(v["interview"].as_array().unwrap().len(), 3, "{v}");
}

#[test]
fn unknown_workflow_refuses_listing_every_workflow() {
    let (code, out) = run(&["route", "--request", "make a video", "--workflow", "nope"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown workflow"), "{out}");
    assert!(out.contains("\"nope\""), "{out}");
    for (_, workflow) in CLEAR_ROUTES {
        assert!(out.contains(workflow), "{out}");
    }
    assert!(out.contains("general-video"), "{out}");
}

#[test]
fn prefs_and_recipes_fill_must_haves_with_sources_named() {
    // An explicit preference answers its slot.
    let v = route_json(&[
        "route",
        "--request",
        "a product launch film for our new headphones",
        "--pref",
        "audience=makers",
    ]);
    let interview = v["interview"].as_array().unwrap();
    assert_eq!(interview.len(), 3, "{v}");
    let audience = interview.iter().find(|q| q["key"] == "audience").unwrap();
    assert_eq!(audience["answered_by"], "preference", "{v}");
    assert_eq!(audience["answer"], "makers", "{v}");
    assert!(
        v["choice"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("preference audience=makers")),
        "choices cite the preference: {v}"
    );
    // A recipe answers what it knows; the rest stay pending.
    let v = route_json(&[
        "route",
        "--request",
        "a voice-free explainer tutorial",
        "--recipe",
        "social-30s",
    ]);
    assert_eq!(v["recipe"], "social-30s", "{v}");
    let interview = v["interview"].as_array().unwrap();
    let duration = interview.iter().find(|q| q["key"] == "duration").unwrap();
    assert_eq!(duration["answered_by"], "recipe social-30s", "{v}");
    assert_eq!(duration["answer"], "30 seconds", "{v}");
    let topic = interview.iter().find(|q| q["key"] == "topic").unwrap();
    assert_eq!(topic["answered_by"], "pending", "{v}");
    assert!(topic["answer"].is_null(), "{v}");
    // Explicit prefs win over the recipe on the same slot.
    let v = route_json(&[
        "route",
        "--request",
        "a voice-free explainer tutorial",
        "--recipe",
        "social-30s",
        "--pref",
        "duration=45 seconds",
    ]);
    let duration = v["interview"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["key"] == "duration")
        .unwrap()
        .clone();
    assert_eq!(duration["answered_by"], "preference", "{v}");
    assert_eq!(duration["answer"], "45 seconds", "{v}");
}

#[test]
fn run_shape_defaults_to_single_and_follows_flags_and_recipes() {
    let v = route_json(&[
        "route",
        "--request",
        "cut vacation clips to our favorite song",
    ]);
    assert_eq!(v["run_shape"], "single", "{v}");
    assert_eq!(v["run_shape_source"], "default", "{v}");
    let v = route_json(&[
        "route",
        "--request",
        "cut vacation clips to our favorite song",
        "--shape",
        "batch",
    ]);
    assert_eq!(v["run_shape"], "batch", "{v}");
    assert!(
        v["run_shape_source"].as_str().unwrap().contains("explicit"),
        "{v}"
    );
    let v = route_json(&[
        "route",
        "--request",
        "cut vacation clips to our favorite song",
        "--recipe",
        "batch-variants",
    ]);
    assert_eq!(v["run_shape"], "batch", "{v}");
    assert!(
        v["run_shape_source"]
            .as_str()
            .unwrap()
            .contains("batch-variants"),
        "{v}"
    );
    // Explicit shape wins over the recipe shape, and the note says so.
    let (code, out) = run(&[
        "route",
        "--request",
        "cut vacation clips to our favorite song",
        "--recipe",
        "batch-variants",
        "--shape",
        "stills-only",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("run shape: stills-only"), "{out}");
    assert!(out.contains("overrides recipe batch-variants"), "{out}");
}

#[test]
fn shape_recipe_and_pref_mistakes_refuse() {
    let (code, _) = run(&["route", "--request", "make a video", "--shape", "imax"]);
    assert_eq!(code, 2);
    let (code, out) = run(&["route", "--request", "make a video", "--recipe", "nope"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown recipe"), "{out}");
    assert!(out.contains("social-30s"), "{out}");
    let (code, _) = run(&["route", "--request", "make a video", "--pref", "audience"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "make a video", "--pref", "=x"]);
    assert_eq!(code, 2);
    let (code, _) = run(&[
        "route",
        "--request",
        "make a video",
        "--pref",
        "a=x",
        "--pref",
        "a=y",
    ]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "make a video", "--install"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "make a video", "--dest", "/tmp/x"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "make a video", "--bogus"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route"]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "   "]);
    assert_eq!(code, 2);
    let (code, _) = run(&["route", "--request", "a", "--request", "b"]);
    assert_eq!(code, 2);
}

#[test]
fn workflows_install_only_when_needed() {
    let root = tmp("install");
    let dest = root.join("skills");
    let dest_s = dest.to_str().unwrap().to_string();
    // First install writes exactly the chosen workflow's files.
    let (code, out) = run(&[
        "route",
        "--request",
        "a product launch film for our new headphones",
        "--install",
        "--host",
        "cursor",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("installed workflow \"product-launch\""),
        "{out}"
    );
    assert!(dest.join("product-launch/SKILL.md").is_file(), "{out}");
    let mut files = Vec::new();
    for entry in walk(&dest) {
        files.push(entry);
    }
    assert_eq!(files, vec!["product-launch/SKILL.md"], "{files:?}");
    // Second run restores nothing: already installed.
    let (code, out) = run(&[
        "route",
        "--request",
        "a product launch film for our new headphones",
        "--install",
        "--host",
        "cursor",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("already installed"), "{out}");
    // Ambiguity installs nothing.
    let (code, out) = run(&[
        "route",
        "--request",
        "make something nice for friday",
        "--install",
        "--host",
        "cursor",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("nothing to install: the request is ambiguous"),
        "{out}"
    );
    // An explicit workflow pick installs just that workflow.
    let (code, out) = run(&[
        "route",
        "--request",
        "make something nice for friday",
        "--workflow",
        "general-video",
        "--install",
        "--host",
        "cursor",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(dest.join("general-video/SKILL.md").is_file(), "{out}");
    assert!(!dest.join("slideshow").exists(), "{out}");
    // Unknown hosts refuse naming the supported hosts.
    let (code, out) = run(&[
        "route",
        "--request",
        "a product launch film for our new headphones",
        "--install",
        "--host",
        "hal9000",
        "--dest",
        &dest_s,
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown host"), "{out}");
    assert!(out.contains("cursor"), "{out}");
}

#[test]
fn brief_informs_the_choice_and_current_words_win() {
    let root = tmp("brief");
    let folder = make_project(&root, "demo");
    let folder_s = folder.to_str().unwrap().to_string();
    std::fs::write(
        folder.join("brief.md"),
        "# Brief\nRequest: a product launch film for our new headphones\n",
    )
    .unwrap();
    // A neutral request still routes on the brief's words.
    let v = route_json(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(v["choice"]["workflow"], "product-launch", "{v}");
    assert_eq!(v["brief"], "ok", "{v}");
    assert!(
        v["choice"]["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.as_str().unwrap().contains("brief.md agrees")),
        "choices cite the brief: {v}"
    );
    // When the brief disagrees, the current words win and the note says so.
    let (code, out) = run(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("brief.md request differs from the current words; the current words win"),
        "{out}"
    );
    // A storyboard on disk is noted without being parsed here.
    std::fs::write(folder.join("storyboard.md"), "# Storyboard\nBoard one.\n").unwrap();
    let v = route_json(&["route", "--request", "make a video", &folder_s]);
    assert!(v["storyboard"].as_str().unwrap().contains("present"), "{v}");
    assert!(
        v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("storyboard.md is present")),
        "{v}"
    );
}

#[test]
fn corrupt_brief_reports_with_a_fresh_offer_and_keeps_the_file() {
    let root = tmp("corrupt");
    let folder = make_project(&root, "demo");
    let folder_s = folder.to_str().unwrap().to_string();
    let brief = folder.join("brief.md");
    std::fs::write(&brief, "these notes never came from an interview\n").unwrap();
    let before = std::fs::read(&brief).unwrap();
    let (code, out) = run(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("corrupt brief"), "{out}");
    assert!(out.contains("brief.md"), "{out}");
    assert!(out.contains("no `Request:` line"), "{out}");
    assert!(out.contains("fresh brief offer"), "{out}");
    assert!(out.contains("What should the video do?"), "{out}");
    assert!(out.contains("re-run with `--fresh`"), "{out}");
    // The corrupt file stays byte-identical for inspection.
    assert_eq!(
        std::fs::read(&brief).unwrap(),
        before,
        "corrupt files are kept"
    );
    // `--fresh` starts the interview without reading project state.
    let v = route_json(&["route", "--request", "make a video", &folder_s, "--fresh"]);
    assert!(v["choice"].is_null(), "{v}");
    assert_eq!(v["brief"], "ignored", "{v}");
    assert_eq!(v["storyboard"], "ignored", "{v}");
    // A binary brief is corrupt the same way.
    std::fs::write(&brief, [0xff, 0xfe, 0x00, 0x28]).unwrap();
    let (code, out) = run(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("corrupt brief"), "{out}");
    assert!(out.contains("not valid UTF-8"), "{out}");
    // An unreadable storyboard (a directory at the file path) refuses too.
    std::fs::remove_file(&brief).unwrap();
    std::fs::create_dir(folder.join("storyboard.md")).unwrap();
    let (code, out) = run(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("storyboard.md"), "{out}");
    assert!(out.contains("left untouched"), "{out}");
}

#[test]
fn missing_folder_refuses() {
    let (code, out) = run(&[
        "route",
        "--request",
        "make a video",
        "/tmp/framesmith-n20-no-such-folder",
    ]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("no project folder"), "{out}");
}

#[test]
fn routing_never_touches_the_project() {
    let root = tmp("untouched");
    let folder = make_project(&root, "demo");
    let folder_s = folder.to_str().unwrap().to_string();
    std::fs::write(
        folder.join("brief.md"),
        "# Brief\nRequest: a photo slideshow of stills\n",
    )
    .unwrap();
    std::fs::write(folder.join("storyboard.md"), "# Storyboard\nBoard one.\n").unwrap();
    let snap_dir = |tag: &str| {
        let out = root.join(format!("snaps-{tag}"));
        let (code, out_text) = run(&[
            "snapshot",
            "--at",
            "0",
            "--out",
            out.to_str().unwrap(),
            &folder_s,
        ]);
        assert_eq!(code, 0, "{out_text}");
        out.join("snap-0s.png")
    };
    let snap_before = snap_dir("before");
    let snap_bytes_before = std::fs::read(&snap_before).unwrap();
    let (code, history_before) = run(&["history", "list", &folder_s]);
    assert_eq!(code, 0);
    let files_before: Vec<(String, Vec<u8>)> = walk(&folder)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(folder.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect();
    // A battery of routes: clear, ambiguous, prefs, shapes, brief,
    // corrupt-then-fresh, and an install into a scratch skill dir.
    let skill_dest = root.join("skills");
    let skill_dest_s = skill_dest.to_str().unwrap().to_string();
    let batteries: Vec<Vec<String>> = vec![
        vec![
            "route".into(),
            "--request".into(),
            "a photo slideshow of stills".into(),
            folder_s.clone(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "make something nice".into(),
            folder_s.clone(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "launch the slideshow".into(),
            folder_s.clone(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "a product launch film".into(),
            "--pref".into(),
            "audience=makers".into(),
            folder_s.clone(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "cut vacation clips to our favorite song".into(),
            "--shape".into(),
            "batch".into(),
            folder_s.clone(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "make a video".into(),
            folder_s.clone(),
            "--fresh".into(),
        ],
        vec![
            "route".into(),
            "--request".into(),
            "recut my talking head interview".into(),
            "--install".into(),
            "--host".into(),
            "cursor".into(),
            "--dest".into(),
            skill_dest_s.clone(),
        ],
    ];
    for args in &batteries {
        let owned: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, out) = run(&owned);
        assert_eq!(code, 0, "{owned:?}:\n{out}");
    }
    // Corrupt the brief, refuse, then recover with --fresh: the
    // refusal path also writes nothing into the project.
    std::fs::write(folder.join("brief.md"), "no request line\n").unwrap();
    let brief_corrupt = std::fs::read(folder.join("brief.md")).unwrap();
    let (code, _) = run(&["route", "--request", "make a video", &folder_s]);
    assert_eq!(code, 1);
    let (code, _) = run(&["route", "--request", "make a video", &folder_s, "--fresh"]);
    assert_eq!(code, 0);
    // Restore the valid brief so the final comparison is like-for-like.
    std::fs::write(
        folder.join("brief.md"),
        "# Brief\nRequest: a photo slideshow of stills\n",
    )
    .unwrap();
    assert_eq!(
        std::fs::read(folder.join("brief.md")).unwrap(),
        files_before
            .iter()
            .find(|(r, _)| r == "brief.md")
            .unwrap()
            .1
    );
    let _ = brief_corrupt;
    let files_after: Vec<(String, Vec<u8>)> = walk(&folder)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(folder.join(&rel)).unwrap();
            (rel, bytes)
        })
        .collect();
    assert_eq!(
        files_before, files_after,
        "routing writes nothing into the project"
    );
    let (code, history_after) = run(&["history", "list", &folder_s]);
    assert_eq!(code, 0);
    assert_eq!(
        history_before, history_after,
        "routing opens no history turns"
    );
    let snap_after = snap_dir("after");
    assert_eq!(
        std::fs::read(&snap_after).unwrap(),
        snap_bytes_before,
        "default frames stay byte-identical"
    );
}

/// Files under one directory, relative, sorted.
fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(top) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&top)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}
