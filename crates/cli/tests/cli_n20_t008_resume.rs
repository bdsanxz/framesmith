//! Sprint 20, T008: resume from brief and storyboard.
//!
//! The router reopens `brief.md`, `storyboard.md`, and the locked-frame
//! gate state on return, continuing from locked frames and offering a
//! fresh interview when no locks exist; the maker's current words win
//! over a stale brief through a staged receipt plus one shared-history
//! turn; corrupt resume files report naming the file, stay on disk for
//! inspection, and never touch the project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n20-t008-{tag}-{}-{}",
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

/// A fresh project with a valid composition: `init demo --dir root`.
fn project(root: &Path) -> PathBuf {
    let (code, out) = run(&["init", "demo", "--dir", root.to_str().unwrap()]);
    assert_eq!(code, 0, "init failed: {out}");
    root.join("demo")
}

fn write(proj: &Path, name: &str, content: &str) {
    std::fs::write(proj.join(name), content).unwrap();
}

fn tree_bytes(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(top) = stack.pop() {
        for entry in std::fs::read_dir(&top).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, std::fs::read(&path).unwrap());
        }
    }
    out
}

fn history_turns(proj: &Path) -> usize {
    let (code, out) = run(&["history", "list", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "history list failed: {out}");
    let line = out.lines().find(|l| l.contains("turn(s)")).unwrap();
    line.split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<usize>()
        .unwrap()
}

/// Persist a presented board exactly like the storyboard slice does.
fn present_board(proj: &Path, approved: bool) {
    let dir = proj.join(".framesmith");
    std::fs::create_dir_all(&dir).unwrap();
    let state = format!(
        concat!(
            "{{\"version\":1,\"composition\":\"demo\",\"title\":\"Launch board\",",
            "\"approved\":{},\"board\":[",
            "{{\"at\":0.4,\"frame\":12,\"label\":\"Opening\",\"still\":\"still-0001.png\"}},",
            "{{\"at\":1.2,\"frame\":36,\"label\":\"Promise\",\"still\":\"still-0002.png\"}}",
            "]}}"
        ),
        approved
    );
    std::fs::write(dir.join("storyboard.json"), state).unwrap();
}

fn gate_path(proj: &Path) -> PathBuf {
    proj.join(".framesmith").join("storyboard.json")
}

const BRIEF: &str = "# Framesmith brief\nRequest: a product launch film about tea for teens\nWorkflow: product-launch\n";

#[test]
fn continue_from_locked_frames_writes_nothing() {
    let root = tmp("continue");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    present_board(&proj, true);
    let before = tree_bytes(&proj);
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume failed: {out}");
    assert!(out.contains("\"Opening\""), "names the first lock: {out}");
    assert!(out.contains("\"Promise\""), "names the second lock: {out}");
    assert!(out.contains("product-launch"), "names the workflow: {out}");
    assert!(out.contains("Launch board"), "names the board: {out}");
    assert!(out.contains("may proceed"), "reports approval: {out}");
    assert_eq!(tree_bytes(&proj), before, "continue writes nothing");
}

#[test]
fn continue_reports_pending_approval() {
    let root = tmp("continue-pending");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    present_board(&proj, false);
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume failed: {out}");
    assert!(
        out.contains("awaits maker approval"),
        "names the pending gate: {out}"
    );
}

#[test]
fn continue_json_names_locks() {
    let root = tmp("continue-json");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    present_board(&proj, false);
    let (code, out) = run(&["resume", "--json", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume --json failed: {out}");
    assert!(out.contains("\"status\":\"continue\""), "{out}");
    assert!(out.contains("Opening") && out.contains("Promise"), "{out}");
    assert!(out.contains("\"approved\":false"), "{out}");
}

#[test]
fn fresh_interview_when_no_board() {
    let root = tmp("fresh-open");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    write(&proj, "storyboard.md", "# Launch board\n\nSome notes.\n");
    let before = tree_bytes(&proj);
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume failed: {out}");
    assert!(
        out.contains("fresh interview"),
        "offers the interview: {out}"
    );
    assert!(
        out.contains("Which product is the film about?"),
        "offers the winning workflow's questions: {out}"
    );
    assert!(
        out.contains("Launch board"),
        "carries the storyboard note: {out}"
    );
    assert_eq!(tree_bytes(&proj), before, "offer writes nothing");
}

#[test]
fn fresh_interview_without_any_files() {
    let root = tmp("fresh-absent");
    let proj = project(&root);
    let before = tree_bytes(&proj);
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume failed: {out}");
    assert!(
        out.contains("fresh interview"),
        "offers the interview: {out}"
    );
    assert!(
        out.contains("general-video"),
        "falls back to general: {out}"
    );
    assert!(out.contains("Not-sure"), "names the Not-sure path: {out}");
    assert_eq!(tree_bytes(&proj), before, "offer writes nothing");
}

#[test]
fn current_words_win_over_stale_brief() {
    let root = tmp("words-win");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    present_board(&proj, true);
    let composition_before = std::fs::read(proj.join("composition.kdl")).unwrap();
    assert_eq!(history_turns(&proj), 0);
    let (code, out) = run(&[
        "resume",
        "--words",
        "a product launch film about coffee for seniors",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "resume --words failed: {out}");
    assert!(out.contains("resume staged"), "staged receipt first: {out}");
    assert!(
        out.contains("resume applied"),
        "applied receipt follows: {out}"
    );
    assert!(
        out.contains("history turn"),
        "names the history turn: {out}"
    );
    let brief = std::fs::read_to_string(proj.join("brief.md")).unwrap();
    assert!(
        brief.contains("Request: a product launch film about coffee for seniors"),
        "current words win: {brief}"
    );
    assert!(
        !brief.contains("tea for teens"),
        "stale words are gone: {brief}"
    );
    assert_eq!(history_turns(&proj), 1, "one agent turn recorded");
    let (_, list) = run(&["history", "list", proj.to_str().unwrap()]);
    assert!(
        list.contains("agent"),
        "turn carries the agent actor: {list}"
    );
    assert_eq!(
        std::fs::read(proj.join("composition.kdl")).unwrap(),
        composition_before,
        "film sources untouched"
    );
}

#[test]
fn words_create_brief_when_absent() {
    let root = tmp("words-create");
    let proj = project(&root);
    let (code, out) = run(&[
        "resume",
        "--words",
        "a slideshow of stills",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "resume --words failed: {out}");
    assert!(out.contains("resume staged"), "staged receipt first: {out}");
    let brief = std::fs::read_to_string(proj.join("brief.md")).unwrap();
    assert!(brief.starts_with("# Framesmith brief\n"), "{brief}");
    assert!(brief.contains("Request: a slideshow of stills"), "{brief}");
    assert_eq!(history_turns(&proj), 1);
}

#[test]
fn in_sync_words_change_nothing() {
    let root = tmp("in-sync");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    let before = tree_bytes(&proj);
    let (code, out) = run(&[
        "resume",
        "--words",
        "a PRODUCT launch film about tea for teens",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "resume failed: {out}");
    assert!(out.contains("already carries"), "reports in-sync: {out}");
    assert_eq!(tree_bytes(&proj), before, "in-sync writes nothing");
    assert_eq!(history_turns(&proj), 0, "no turn for agreement");
}

#[test]
fn corrupt_brief_reports_and_stays() {
    let root = tmp("corrupt-brief");
    let proj = project(&root);
    write(&proj, "brief.md", "just some notes\nno request line\n");
    present_board(&proj, true);
    let brief_before = std::fs::read(proj.join("brief.md")).unwrap();
    let composition_before = std::fs::read(proj.join("composition.kdl")).unwrap();
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "corrupt brief must refuse: {out}");
    assert!(out.contains("brief.md"), "names the file: {out}");
    assert!(
        out.contains("no `Request:` line"),
        "names the reason: {out}"
    );
    assert!(
        out.contains("fresh brief offer"),
        "offers a fresh brief: {out}"
    );
    assert!(out.contains("untouched"), "promises no writes: {out}");
    assert_eq!(
        std::fs::read(proj.join("brief.md")).unwrap(),
        brief_before,
        "corrupt file kept for inspection"
    );
    assert_eq!(
        std::fs::read(proj.join("composition.kdl")).unwrap(),
        composition_before,
        "film untouched"
    );
    assert_eq!(history_turns(&proj), 0, "no turn on failure");
}

#[test]
fn unreadable_storyboard_note_reports_and_stays() {
    let root = tmp("corrupt-story");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    std::fs::write(proj.join("storyboard.md"), [0xff, 0xfe, 0x00]).unwrap();
    let story_before = std::fs::read(proj.join("storyboard.md")).unwrap();
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "unreadable storyboard must refuse: {out}");
    assert!(out.contains("storyboard.md"), "names the file: {out}");
    assert!(
        out.contains("fresh brief offer"),
        "offers a fresh brief: {out}"
    );
    assert_eq!(
        std::fs::read(proj.join("storyboard.md")).unwrap(),
        story_before,
        "corrupt file kept for inspection"
    );
    assert_eq!(history_turns(&proj), 0, "no turn on failure");
}

#[test]
fn corrupt_gate_state_reports_and_stays() {
    let root = tmp("corrupt-gate");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    let dir = proj.join(".framesmith");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("storyboard.json"), "{ not json").unwrap();
    let gate_before = std::fs::read(gate_path(&proj)).unwrap();
    let before = tree_bytes(&proj);
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "corrupt gate state must refuse: {out}");
    assert!(out.contains("storyboard.json"), "names the file: {out}");
    assert!(
        out.contains("fresh brief offer"),
        "offers a fresh brief: {out}"
    );
    assert_eq!(
        std::fs::read(gate_path(&proj)).unwrap(),
        gate_before,
        "corrupt file kept for inspection"
    );
    assert_eq!(tree_bytes(&proj), before, "resume failure touches nothing");
    assert_eq!(history_turns(&proj), 0, "no turn on failure");
}

#[test]
fn corrupt_brief_with_words_still_refuses() {
    let root = tmp("corrupt-words");
    let proj = project(&root);
    write(&proj, "brief.md", "notes without a request line\n");
    let brief_before = std::fs::read(proj.join("brief.md")).unwrap();
    let (code, out) = run(&["resume", "--words", "a launch film", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "corrupt brief must refuse even with words: {out}");
    assert_eq!(
        std::fs::read(proj.join("brief.md")).unwrap(),
        brief_before,
        "nothing written on failure"
    );
    assert_eq!(history_turns(&proj), 0);
}

#[test]
fn corrupt_json_reports_status() {
    let root = tmp("corrupt-json");
    let proj = project(&root);
    write(&proj, "brief.md", "notes without a request line\n");
    let (code, out) = run(&["resume", "--json", proj.to_str().unwrap()]);
    assert_eq!(code, 1, "corrupt brief must refuse: {out}");
    assert!(out.contains("\"status\":\"corrupt\""), "{out}");
    assert!(out.contains("brief.md"), "{out}");
}

#[test]
fn missing_folder_refuses() {
    let root = tmp("missing");
    let gone = root.join("nope");
    let (code, out) = run(&["resume", gone.to_str().unwrap()]);
    assert_eq!(code, 1, "missing folder must refuse: {out}");
    assert!(out.contains("no project directory"), "{out}");
}

#[test]
fn guard_held_refuses_without_writing() {
    let root = tmp("guard");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    let (code, out) = run(&[
        "history",
        "begin",
        "--who",
        "maker",
        "--exclusive",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "begin failed: {out}");
    let brief_before = std::fs::read(proj.join("brief.md")).unwrap();
    let (code, out) = run(&[
        "resume",
        "--words",
        "a different film",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "guard-held resume must refuse: {out}");
    assert!(
        out.contains("resume refused"),
        "refusal names resume: {out}"
    );
    assert_eq!(
        std::fs::read(proj.join("brief.md")).unwrap(),
        brief_before,
        "guard-held run writes nothing"
    );
    let (code, _) = run(&["history", "end", "--who", "maker", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "end failed");
}

#[test]
fn usage_errors_exit_2() {
    let root = tmp("usage");
    let proj = project(&root);
    let (code, out) = run(&["resume", "--bogus", proj.to_str().unwrap()]);
    assert_eq!(code, 2, "unknown flag is usage: {out}");
    let (code, out) = run(&["resume", "--words", "   ", proj.to_str().unwrap()]);
    assert_eq!(code, 2, "empty words are usage: {out}");
    let (code, _) = run(&["resume"]);
    assert_eq!(code, 2, "missing folder is usage");
}

#[test]
fn renders_stay_identical_across_resume() {
    let root = tmp("render-parity");
    let proj = project(&root);
    write(&proj, "brief.md", BRIEF);
    present_board(&proj, true);
    let out_mp4 = |tag: &str| root.join(format!("{tag}.mp4"));
    let a = out_mp4("a");
    let (code, out) = run(&[
        "render",
        proj.to_str().unwrap(),
        "-o",
        a.to_str().unwrap(),
        "--backend",
        "cpu",
    ]);
    assert_eq!(code, 0, "first render failed: {out}");
    let bytes_a = std::fs::read(&a).unwrap();
    let (code, out) = run(&["resume", proj.to_str().unwrap()]);
    assert_eq!(code, 0, "resume failed: {out}");
    let (code, out) = run(&[
        "resume",
        "--words",
        "a product launch film about coffee for seniors",
        proj.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "resume --words failed: {out}");
    let b = out_mp4("b");
    let (code, out) = run(&[
        "render",
        proj.to_str().unwrap(),
        "-o",
        b.to_str().unwrap(),
        "--backend",
        "cpu",
    ]);
    assert_eq!(code, 0, "second render failed: {out}");
    assert_eq!(
        std::fs::read(&b).unwrap(),
        bytes_a,
        "default renders stay byte-identical across resume"
    );
}
