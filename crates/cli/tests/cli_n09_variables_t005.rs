//! Sprint 9 T005 end-to-end: `--theme` on `check` and `render`
//! through the built binary — one selection re-skinning the render,
//! CLI beating the composition attribute, override beating theme,
//! and theme-level refusals before frame one.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n09t005-{}-{}-{}",
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

const PROJ: &str = "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\ntoken id=\"title\" value=\"Brand\"\n}\ntheme name=\"bold\" {\ntoken id=\"accent\" value=\"#00ff00\"\ntoken id=\"title\" value=\"Bold\"\n}\ncomposition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 theme=\"brand\" {\nvariable id=\"accent\" type=\"color\" default=\"#000000\"\nvariable id=\"title\" type=\"string\" default=\"News\"\ntext value=\"var(--title)\" start=0 duration=1 color=\"var(--accent)\"\n}\n";

fn setup(tag: &str) -> PathBuf {
    let root = tmp(tag);
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(&root.join("proj"), "composition.kdl", PROJ);
    root
}

#[test]
fn check_accepts_theme_selection_and_stays_green() {
    let root = setup("check-ok");
    let (code, out) = run(&["check", "--theme", "brand", "proj"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("ok:"), "{out}");
}

#[test]
fn render_reskins_under_two_themes_plus_an_override() {
    let root = setup("render-reskin");
    let before = std::fs::read(root.join("proj").join("composition.kdl")).unwrap();

    let (code, out) = run(
        &["render", "proj", "-o", "a.mp4", "--theme", "brand"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(&["render", "proj", "-o", "b.mp4", "--theme", "bold"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &[
            "render",
            "proj",
            "-o",
            "c.mp4",
            "--theme",
            "brand",
            "--variables",
            r##"{"title": "Sale"}"##,
        ],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    // Same theme re-renders byte-identically (determinism per variant).
    let (code, out) = run(
        &["render", "proj", "-o", "a2.mp4", "--theme", "brand"],
        &root,
    );
    assert_eq!(code, 0, "{out}");

    let a = std::fs::read(root.join("a.mp4")).unwrap();
    let b = std::fs::read(root.join("b.mp4")).unwrap();
    let c = std::fs::read(root.join("c.mp4")).unwrap();
    let a2 = std::fs::read(root.join("a2.mp4")).unwrap();
    assert!(!a.is_empty() && !b.is_empty() && !c.is_empty());
    assert_ne!(a, b, "two themes must render different outputs");
    assert_ne!(
        a, c,
        "an override on top of a theme must render differently"
    );
    assert_eq!(a, a2, "the same theme must re-render byte-identically");

    let after = std::fs::read(root.join("proj").join("composition.kdl")).unwrap();
    assert_eq!(
        before, after,
        "themed renders must leave the project on disk unchanged"
    );
}

#[test]
fn cli_theme_beats_the_composition_theme_attribute() {
    // The composition selects `brand`; the CLI picks `bold`.
    let root = setup("cli-beats-attr");
    let (code, out) = run(&["render", "proj", "-o", "attr.mp4"], &root);
    assert_eq!(code, 0, "{out}");
    let (code, out) = run(
        &["render", "proj", "-o", "cli.mp4", "--theme", "bold"],
        &root,
    );
    assert_eq!(code, 0, "{out}");
    let attr = std::fs::read(root.join("attr.mp4")).unwrap();
    let cli = std::fs::read(root.join("cli.mp4")).unwrap();
    assert_ne!(
        attr, cli,
        "the CLI theme must beat the composition attribute"
    );
}

#[test]
fn check_refuses_two_themes_naming_the_rule() {
    let root = tmp("two-themes");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(
        &root.join("proj"),
        "composition.kdl",
        &PROJ.replace("theme=\"brand\"", "theme=\"brand bold\""),
    );
    let (code, out) = run(&["check", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("at most one theme"), "{out}");
}

#[test]
fn check_refuses_unknown_theme_names_naming_the_available() {
    let root = setup("unknown-theme");
    let (code, out) = run(&["check", "--theme", "ghost", "proj"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--theme"), "{out}");
    assert!(out.contains("ghost"), "{out}");
    assert!(out.contains("available themes"), "{out}");
    assert!(out.contains("\"brand\""), "{out}");
}

#[test]
fn render_refuses_unknown_theme_before_frame_one_with_no_mp4() {
    let root = setup("render-unknown");
    let (code, out) = run(
        &["render", "proj", "-o", "nope.mp4", "--theme", "ghost"],
        &root,
    );
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("ghost"), "{out}");
    assert!(out.contains("available themes"), "{out}");
    assert!(!root.join("nope.mp4").exists());
}

#[test]
fn render_refuses_a_call_resolving_through_nothing() {
    let root = tmp("through-nothing");
    std::fs::create_dir_all(root.join("proj")).unwrap();
    write(
        &root.join("proj"),
        "composition.kdl",
        "theme name=\"brand\" {\ntoken id=\"accent\" value=\"#ff3366\"\n}\ncomposition name=\"t\" width=64 height=64 duration=1 background=\"#000\" version=1 theme=\"brand\" {\ntext value=\"Hi var(--x)\" start=0 duration=1\n}\n",
    );
    let (code, out) = run(&["render", "proj", "-o", "x.mp4"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("--x"), "{out}");
    assert!(out.contains("token"), "{out}");
    assert!(!root.join("x.mp4").exists());
}
