//! Sprint 9 T006 end-to-end: `render --batch` through the built
//! binary — one MP4 per rows-file row, templated names, fail-fast on
//! and off, the per-row summary, kept MP4s, exit codes, and
//! byte-identical re-renders.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n09t006-{}-{}-{}",
        tag,
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

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

const PROJ: &str = "composition name=\"t\" width=64 height=64 duration=1 fps=30 background=\"#000\" version=1 {\nvariable id=\"title\" type=\"string\" default=\"News\"\nvariable id=\"shade\" type=\"enum\" default=\"red\" options=\"red, blue\"\ntext value=\"var(--title) var(--shade)\" start=0 duration=1 color=\"#ffffff\"\n}\n";

fn setup(tag: &str) -> PathBuf {
    let root = tmp(tag);
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    root
}

#[test]
fn batch_renders_every_row_to_its_templated_name() {
    let root = setup("all-pass");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "Alpha"}}, {"variables": {"title": "Beta"}}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{title}-{index}",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("batch: row 0: ok"), "{out}");
    assert!(out.contains("batch: row 1: ok"), "{out}");
    assert!(out.contains("batch summary: 2 row(s): 2 passed"), "{out}");

    let a = std::fs::read(root.join("proj").join("clip-Alpha-0.mp4")).unwrap();
    let b = std::fs::read(root.join("proj").join("clip-Beta-1.mp4")).unwrap();
    assert!(!a.is_empty() && !b.is_empty());
    assert_ne!(a, b, "different titles must render different outputs");

    let before = PROJ.to_string();
    let after = std::fs::read_to_string(root.join("proj").join("composition.kdl")).unwrap();
    assert_eq!(before, after, "batch renders leave the project unchanged");
}

#[test]
fn batch_re_renders_byte_identical_per_row() {
    let root = setup("byte-identical");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "One"}}, {"variables": {"title": "Two"}}]"#,
    );
    let args = [
        "render",
        "proj",
        "--batch",
        "rows.json",
        "--name-template",
        "clip-{title}-{index}",
    ];
    let (code, out) = run(&args, &root);
    assert_eq!(code, 0, "{out}");
    let a = std::fs::read(root.join("proj").join("clip-One-0.mp4")).unwrap();
    let b = std::fs::read(root.join("proj").join("clip-Two-1.mp4")).unwrap();
    let (code, out) = run(&args, &root);
    assert_eq!(code, 0, "{out}");
    let a2 = std::fs::read(root.join("proj").join("clip-One-0.mp4")).unwrap();
    let b2 = std::fs::read(root.join("proj").join("clip-Two-1.mp4")).unwrap();
    assert_eq!(a, a2, "the same rows file re-renders byte-identically");
    assert_eq!(b, b2, "the same rows file re-renders byte-identically");
}

#[test]
fn batch_fail_fast_stops_at_the_bad_row_and_keeps_earlier_mp4s() {
    let root = setup("fail-fast");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "Good"}}, {"variables": {"shade": "green"}}, {"variables": {"title": "Later"}}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{title}-{index}",
            "--fail-fast",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("batch: row 0: ok"), "{out}");
    assert!(out.contains("batch: row 1: failed"), "{out}");
    assert!(out.contains("row 1"), "{out}");
    assert!(out.contains("skipped"), "{out}");
    assert!(out.contains("1 passed, 1 failed, 1 skipped"), "{out}");
    // Earlier rows' MP4s stay in place; the later row never renders.
    assert!(root.join("proj").join("clip-Good-0.mp4").exists());
    assert!(!root.join("proj").join("clip--1.mp4").exists());
    assert!(!root.join("proj").join("clip-Later-2.mp4").exists());
}

#[test]
fn batch_without_fail_fast_attempts_every_row() {
    let root = setup("no-fail-fast");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "Good"}}, {"variables": {"shade": "green"}}, {"variables": {"title": "Later"}}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{title}-{index}",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("batch: row 0: ok"), "{out}");
    assert!(out.contains("batch: row 1: failed"), "{out}");
    assert!(out.contains("batch: row 2: ok"), "{out}");
    assert!(out.contains("2 passed, 1 failed, 0 skipped"), "{out}");
    assert!(root.join("proj").join("clip-Good-0.mp4").exists());
    assert!(root.join("proj").join("clip-Later-2.mp4").exists());
}

#[test]
fn batch_jobs_two_matches_sequential_outputs() {
    let root = setup("jobs");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "Jax"}}, {"variables": {"title": "Kay"}}]"#,
    );
    let seq = [
        "render",
        "proj",
        "--batch",
        "rows.json",
        "--name-template",
        "seq-{title}-{index}",
    ];
    let (code, out) = run(&seq, &root);
    assert_eq!(code, 0, "{out}");
    let par = [
        "render",
        "proj",
        "--batch",
        "rows.json",
        "--name-template",
        "par-{title}-{index}",
        "--jobs",
        "2",
    ];
    let (code, out) = run(&par, &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("batch summary: 2 row(s): 2 passed"), "{out}");
    // Same titles under different names render identical bytes: the
    // parallel schedule changes nothing per row.
    let s0 = std::fs::read(root.join("proj").join("seq-Jax-0.mp4")).unwrap();
    let p0 = std::fs::read(root.join("proj").join("par-Jax-0.mp4")).unwrap();
    let s1 = std::fs::read(root.join("proj").join("seq-Kay-1.mp4")).unwrap();
    let p1 = std::fs::read(root.join("proj").join("par-Kay-1.mp4")).unwrap();
    assert_eq!(s0, p0);
    assert_eq!(s1, p1);
}

#[test]
fn batch_refuses_unreadable_rows_file_naming_the_problem() {
    let root = setup("unreadable");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "missing.json",
            "--name-template",
            "clip-{index}",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("cannot read batch rows file"), "{out}");
    assert!(out.contains("missing.json"), "{out}");
}

#[test]
fn batch_refuses_bad_row_shape_naming_the_file_problem() {
    let root = setup("bad-shape");
    write(&root, "rows.json", r#"[{"variables": {"title": ["x"]}}]"#);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{index}",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("bad row shape"), "{out}");
    assert!(out.contains("rows.json"), "{out}");
    assert!(out.contains("row 0"), "{out}");
}

#[test]
fn batch_refuses_unknown_placeholder_before_any_render() {
    let root = setup("unknown-placeholder");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "A"}}, {"variables": {"title": "B"}}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{bogus}-{index}",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("unknown placeholder {bogus}"), "{out}");
    assert!(!root.join("proj").join("clip-A-0.mp4").exists());
    assert!(!root.join("proj").join("clip-B-1.mp4").exists());
}

#[test]
fn batch_refuses_duplicate_output_names_naming_both_rows() {
    let root = setup("duplicate-names");
    write(
        &root,
        "rows.json",
        r#"[{"variables": {"title": "A"}}, {"variables": {"title": "B"}}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "same",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("rows 0 and 1"), "{out}");
    assert!(out.contains("same output name"), "{out}");
    assert!(!root.join("proj").join("same.mp4").exists());
}

#[test]
fn batch_refuses_rows_above_the_limit_naming_it() {
    let root = setup("over-limit");
    let mut body = String::from("[");
    for i in 0..10001 {
        if i > 0 {
            body.push(',');
        }
        body.push_str(r#"{"variables": {}}"#);
    }
    body.push(']');
    write(&root, "rows.json", &body);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{index}",
        ],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("above the limit"), "{out}");
    assert!(out.contains("10000"), "{out}");
}

#[test]
fn batch_flag_misuse_is_a_usage_error() {
    let root = setup("misuse");
    // Batch-only flags without --batch.
    let (code, out) = run(&["render", "proj", "--jobs", "2"], &root);
    assert_eq!(code, 2, "{out}");
    // --output names one file; batch names come from the template.
    write(&root, "rows.json", r#"[]"#);
    let (code, out) = run(
        &["render", "proj", "-o", "out.mp4", "--batch", "rows.json"],
        &root,
    );
    assert_eq!(code, 2, "{out}");
    // Rows carry the variables; a global payload would be ambiguous.
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--variables",
            r#"{"title": "X"}"#,
        ],
        &root,
    );
    assert_eq!(code, 2, "{out}");
}

#[test]
fn batch_reports_zero_rows_and_exits_zero() {
    let root = setup("zero-rows");
    write(&root, "rows.json", r#"[]"#);
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "clip-{index}",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("0 row(s)"), "{out}");
}

#[test]
fn batch_rows_can_select_their_own_theme() {
    let root = tmp("row-theme");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(
        &root.join("proj"),
        "composition.kdl",
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\ntoken id=\"title\" value=\"Brand\"\n}\ntheme name=\"bold\" {\ntoken id=\"accent\" value=\"#00ff00\"\ntoken id=\"title\" value=\"Bold\"\n}\ncomposition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=1 color=\"var(--accent)\"\n}\n",
    );
    write(
        &root,
        "rows.json",
        r#"[{"variables": {}, "theme": "brand"}, {"variables": {}, "theme": "bold"}]"#,
    );
    let (code, out) = run(
        &[
            "render",
            "proj",
            "--batch",
            "rows.json",
            "--name-template",
            "row-{index}",
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let brand = std::fs::read(root.join("proj").join("row-0.mp4")).unwrap();
    let bold = std::fs::read(root.join("proj").join("row-1.mp4")).unwrap();
    assert_ne!(brand, bold, "per-row themes must re-skin the rows");
}
