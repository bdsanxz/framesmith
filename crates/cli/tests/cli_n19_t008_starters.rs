//! Sprint 19 T008: start projects from bundled starter examples.
//!
//! `init --example <starter>` creates a ready project (composition plus
//! agent guide) that checks, opens via `compositions`, and snapshots to
//! its recorded preview digest untouched. Refusals (unknown example,
//! non-empty target, bad names, bad flags) change nothing.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n19-t008-{tag}-{}-{}",
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

/// (starter, preview digest): the digest is the SHA256 of the untouched
/// starter's `snapshot --at 0` PNG bytes (default CPU backend).
const STARTER_DIGESTS: &[(&str, &str)] = &[
    (
        "title-card",
        "2c70dd83cdcc85012c78c9e53cf9f3622545c86daa814887c8c07f7a6f749f69",
    ),
    (
        "lower-third",
        "621a04b26f5abad83ea10a489d17c26f3fe5ea9233e75fb745410fc7d00d1b5d",
    ),
    (
        "color-grid",
        "d041656284e8ba55b5b32b5d5606f0a9dad81f74ef4ac2e015fa9c07fc2950ca",
    ),
];

#[test]
fn named_starters_create_ready_projects() {
    for (starter, _) in STARTER_DIGESTS {
        let root = tmp(starter);
        let proj = format!("from-{starter}");
        let (code, out) = run(&["init", &proj, "--example", starter], &root);
        assert_eq!(code, 0, "{out}");
        assert!(out.contains(starter), "init should name the starter: {out}");
        let folder = root.join(&proj);
        // Ready project: composition plus the agent guide file.
        assert!(folder.join("composition.kdl").is_file(), "{out}");
        let guide = folder.join("AGENTS.md");
        assert!(guide.is_file(), "{out}");
        assert!(
            std::fs::read_to_string(&guide).unwrap().contains(*starter),
            "guide should name its starter"
        );
        // Ready to open in Studio: the composition lists and validates.
        let (code, out) = run(&["compositions", &proj], &root);
        assert_eq!(code, 0, "{out}");
        let (code, out) = run(&["check", &proj], &root);
        assert_eq!(code, 0, "{out}");
    }
}

#[test]
fn untouched_starters_render_matching_preview_digests() {
    for (starter, digest) in STARTER_DIGESTS {
        let root = tmp(starter);
        let proj = format!("snap-{starter}");
        let (code, out) = run(&["init", &proj, "--example", starter], &root);
        assert_eq!(code, 0, "{out}");
        let out_dir = root.join(format!("out-{starter}"));
        std::fs::create_dir_all(&out_dir).unwrap();
        let out_arg = out_dir.to_string_lossy().into_owned();
        let (code, out) = run(&["snapshot", "--at", "0", "--out", &out_arg, &proj], &root);
        assert_eq!(code, 0, "{out}");
        let png = out_dir.join("snap-0s.png");
        assert!(png.is_file(), "snapshot must write the frame PNG: {out}");
        let bytes = std::fs::read(&png).unwrap();
        assert_eq!(
            ffmpeg::sha256::sha256_hex(&bytes),
            *digest,
            "untouched starter {starter:?} must render to its preview digest"
        );
    }
}

#[test]
fn minimal_init_gains_guide_and_renders_unchanged() {
    let root = tmp("minimal");
    let (code, out) = run(&["init", "plain"], &root);
    assert_eq!(code, 0, "{out}");
    assert!(root.join("plain").join("AGENTS.md").is_file(), "{out}");
    let (code, out) = run(&["check", "plain"], &root);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn unknown_example_refuses_listing_available_starters() {
    let root = tmp("unknown");
    let (code, out) = run(&["init", "ghost", "--example", "no-such-starter"], &root);
    assert_eq!(code, 1, "{out}");
    for (starter, _) in STARTER_DIGESTS {
        assert!(
            out.contains(*starter),
            "refusal must list starter {starter:?}: {out}"
        );
    }
    assert!(
        !root.join("ghost").exists(),
        "unknown example must create nothing: {out}"
    );
}

#[test]
fn nonempty_target_refuses_with_nothing_created() {
    let root = tmp("occupied");
    let target = root.join("taken");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("notes.txt"), "do not touch").unwrap();
    std::fs::write(target.join("composition.kdl"), "keep me").unwrap();
    let before: Vec<(PathBuf, Vec<u8>)> = {
        let mut v: Vec<_> = std::fs::read_dir(&target)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.clone(), std::fs::read(&p).unwrap())
            })
            .collect();
        v.sort();
        v
    };
    let (code, out) = run(&["init", "taken", "--example", "title-card"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("not empty"), "{out}");
    let mut after: Vec<_> = std::fs::read_dir(&target)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), std::fs::read(&p).unwrap())
        })
        .collect();
    after.sort();
    assert_eq!(before, after, "occupied target must stay byte-identical");
}

#[test]
fn existing_init_flags_and_refusals_keep_working() {
    let root = tmp("flags");
    // --dir still routes the project.
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let sub_arg = sub.to_string_lossy().into_owned();
    let (code, out) = run(&["init", "via-dir", "--dir", &sub_arg], &root);
    assert_eq!(code, 0, "{out}");
    assert!(sub.join("via-dir").join("composition.kdl").is_file());
    // Bad names still refuse before touching anything.
    let (code, out) = run(&["init", "has space", "--example", "title-card"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("invalid project name"), "{out}");
    assert!(!root.join("has space").exists());
    // Unknown flags, missing values, and missing names stay usage errors.
    let (code, _) = run(&["init", "x", "--nope"], &root);
    assert_eq!(code, 2);
    let (code, _) = run(&["init", "x", "--example"], &root);
    assert_eq!(code, 2);
    let (code, _) = run(&["init", "--example", "title-card"], &root);
    assert_eq!(code, 2);
    // Gate order: the name refusal wins over an unknown example.
    let (code, out) = run(&["init", "bad name", "--example", "nope"], &root);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("invalid project name"), "{out}");
}
