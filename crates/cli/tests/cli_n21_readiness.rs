//! Sprint 21 T007: adapter readiness and lint parity end-to-end
//! through the `framesmith check`, `lint`, and `render` binaries.
//!
//! Covers the user-visible half of `T007-report-readiness-and-lint.md`:
//! `check` lists every adapter source as ready/loading/failed with
//! cause, per source and never per frame; `lint` accepts adapter
//! sources and refuses broken assets, ranges, and attributions
//! naming the file and cause; a failed source blocks the render with
//! the same file the inspector names; adapter-free output is
//! unchanged.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-e2e-n21readiness-{tag}-{}-{}",
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
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(cwd);
    let out = cmd.output().expect("run framesmith binary");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

fn write(folder: &Path, name: &str, content: &[u8]) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn slide_json() -> Vec<u8> {
    serde_json::json!({
        "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
        "nm": "slide",
        "markers": [{"tm": 0.0, "cm": "intro", "dr": 30.0}],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
            "ks": {"o": {"a": 0, "k": 100.0}, "r": {"a": 0, "k": 0.0},
                "p": {"a": 1, "k": [
                    {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                    {"t": 30.0, "s": [50.0, 32.0, 0.0]}]},
                "a": {"a": 0, "k": [0.0, 0.0, 0.0]}, "s": {"a": 0, "k": [100.0, 100.0, 100.0]}},
            "shapes": [{"ty": "gr", "it": [
                {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                 "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}]}]}]
    })
    .to_string()
    .into_bytes()
}

// --- minimal tile-pack fixture (hand-encoded solid PNGs) ---

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn chunk(kind: &[u8; 4], data: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut c = Vec::new();
    c.extend_from_slice(kind);
    c.extend_from_slice(data);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

fn solid_png(edge: u32, color: (u8, u8, u8)) -> Vec<u8> {
    let mut raw = Vec::new();
    for _ in 0..edge {
        raw.push(0);
        for _ in 0..edge {
            raw.extend_from_slice(&[color.0, color.1, color.2]);
        }
    }
    let mut deflate = vec![0x78, 0x01];
    let mut rest = raw.as_slice();
    while !rest.is_empty() {
        let take = rest.len().min(65535);
        let (head, tail) = rest.split_at(take);
        rest = tail;
        deflate.push(u8::from(rest.is_empty()));
        deflate.extend_from_slice(&(take as u16).to_le_bytes());
        deflate.extend_from_slice(&(!(take as u16)).to_le_bytes());
        deflate.extend_from_slice(head);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    deflate.extend_from_slice(&((b << 16 | a).to_be_bytes()));
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&edge.to_be_bytes());
    ihdr.extend_from_slice(&edge.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr, &mut png);
    chunk(b"IDAT", &deflate, &mut png);
    chunk(b"IEND", &[], &mut png);
    png
}

fn full_pack(folder: &Path) {
    for x in 0..2 {
        for y in 0..2 {
            let rel = format!("tiles/1/{x}/{y}.png");
            let abs = folder.join(&rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(abs, solid_png(256, (200, 210, 220))).unwrap();
        }
    }
    write(
        folder,
        "tiles/pack.json",
        "{\"attribution\": \"© fixtures\", \"min-zoom\": 1, \"max-zoom\": 3}".as_bytes(),
    );
}

fn proj(root: &Path, body: &[u8]) -> PathBuf {
    let folder = root.join("proj");
    std::fs::create_dir_all(&folder).unwrap();
    write(&folder, "composition.kdl", body);
    folder
}

fn mixed_body() -> Vec<u8> {
    // Clips sit on disjoint canvas rects, so the static lint batch
    // stays silent and the readiness listing speaks alone.
    "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    lottie id=\"hero\" path=\"anim.json\" start=0 duration=2 width=64 height=64\n    chart kind=\"bar\" start=0 duration=2 x=330 y=260 width=300 height=200 {\n        row label=\"Q1\" value=12\n    }\n    scene3d start=0 duration=2 width=320 height=240 x=0 y=240 {\n    object name=\"cube\" kind=\"box\"\n}\n    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 x=384 y=0 start=0 duration=2 {\n    }\n}\n"
        .into()
}

#[test]
fn check_lists_readiness_across_kinds() {
    let root = tmp("mixed");
    let folder = proj(&root, &mixed_body());
    write(&folder, "anim.json", &slide_json());
    full_pack(&folder);
    let (code, text) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "mixed project checks clean:\n{text}");
    // One line per source, per source and never per frame.
    assert!(
        text.contains("readiness[lottie] lottie \"hero\": ready (anim.json)"),
        "lottie lists ready:\n{text}"
    );
    assert!(
        text.contains("readiness[chart] <chart clip 2>: ready (inline rows)"),
        "chart lists ready:\n{text}"
    );
    assert!(
        text.contains("readiness[scene3d] <scene3d clip 3>: ready (authored scene)"),
        "scene lists ready:\n{text}"
    );
    assert!(
        text.contains("readiness[map] <map clip 4>: ready (tiles)"),
        "map lists ready:\n{text}"
    );
    assert!(
        text.contains("readiness: 4 source(s): 4 ready, 0 loading, 0 failed."),
        "the tally lists once:\n{text}"
    );
}

#[test]
fn check_and_lint_json_carry_the_same_readiness() {
    let root = tmp("json");
    let folder = proj(&root, &mixed_body());
    write(&folder, "anim.json", &slide_json());
    full_pack(&folder);
    let (code, text) = run(&["check", "--json", "proj"], &root);
    assert_eq!(code, 0, "check --json passes:\n{text}");
    assert!(
        text.contains("\"readiness\":[")
            && text.contains("\"kind\":\"lottie\"")
            && text.contains("\"kind\":\"chart\"")
            && text.contains("\"kind\":\"scene3d\"")
            && text.contains("\"kind\":\"map\"")
            && text.contains("\"state\":\"ready\""),
        "check --json carries every source as ready:\n{text}"
    );
    let (code, text) = run(&["lint", "--json", "proj"], &root);
    assert_eq!(code, 0, "lint --json passes:\n{text}");
    assert!(
        text.contains("\"readiness\":[")
            && text.contains("\"kind\":\"lottie\"")
            && text.contains("\"kind\":\"map\"")
            && text.contains("\"state\":\"ready\""),
        "lint --json carries the same readiness:\n{text}"
    );
    // The lint registry holds fourteen stable rules.
    assert!(
        text.contains("\"lint_rules\":14"),
        "no rule was invented for adapters:\n{text}"
    );
}

#[test]
fn lint_accepts_valid_adapter_project() {
    let root = tmp("lint-ok");
    let folder = proj(&root, &mixed_body());
    write(&folder, "anim.json", &slide_json());
    full_pack(&folder);
    let (code, text) = run(&["lint", "proj"], &root);
    assert_eq!(code, 0, "lint accepts adapter sources:\n{text}");
    assert!(
        text.contains("readiness: 4 source(s): 4 ready, 0 loading, 0 failed."),
        "lint lists the same readiness as check:\n{text}"
    );
    assert!(
        text.contains("lint: 0 findings across 14 rules"),
        "the engine is unchanged for adapters:\n{text}"
    );
}

#[test]
fn lint_refuses_a_broken_asset_naming_file_and_cause() {
    let root = tmp("lint-bad");
    write(&root, "bad.json", b"{oops");
    proj(
        &root,
        "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    lottie path=\"../bad.json\" start=0 duration=1 width=64 height=64\n}\n"
            .as_bytes(),
    );
    let (code, text) = run(&["lint", "proj"], &root);
    assert_ne!(code, 0, "broken asset refuses:\n{text}");
    assert!(
        text.contains("bad.json") && text.contains("E_BAD_LOTTIE"),
        "lint names the file and the code:\n{text}"
    );
}

#[test]
fn lint_refuses_a_missing_attribution_naming_the_pack() {
    let root = tmp("lint-attribution");
    let folder = proj(
        &root,
        "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    map pack=\"tiles\" center=\"0,0\" zoom=1 width=256 height=256 start=0 duration=2 {\n    }\n}\n"
            .as_bytes(),
    );
    std::fs::create_dir_all(folder.join("tiles/1/0")).unwrap();
    write(&folder, "tiles/1/0/0.png", &solid_png(256, (1, 2, 3)));
    let (code, text) = run(&["lint", "proj"], &root);
    assert_ne!(code, 0, "attribution-less pack refuses:\n{text}");
    assert!(
        text.contains("tiles") && text.contains("E_BAD_MAP") && text.contains("attribution"),
        "lint names the pack and the attribution cause:\n{text}"
    );
}

#[test]
fn failed_source_lists_in_check_and_blocks_the_render() {
    let root = tmp("gate");
    write(&root, "bad.json", b"{oops");
    proj(
        &root,
        "composition name=\"main\" width=64 height=64 duration=4 background=\"#000\" version=1 {\n    lottie path=\"../bad.json\" start=0 duration=1 width=64 height=64\n}\n"
            .as_bytes(),
    );
    // The inspector names the failed source with file and cause.
    let (code, text) = run(&["check", "proj"], &root);
    assert_ne!(code, 0, "failed source refuses:\n{text}");
    assert!(
        text.contains("readiness[lottie] <lottie clip 1>: failed")
            && text.contains("bad.json")
            && text.contains("undecodable"),
        "check lists the failed source with cause:\n{text}"
    );
    // The render path blocks on the same file: no MP4 is written.
    let (code, text) = run(&["render", "proj"], &root);
    assert_ne!(code, 0, "render refuses the failed source:\n{text}");
    assert!(
        text.contains("bad.json"),
        "the render gate names the same file:\n{text}"
    );
    assert_eq!(
        std::fs::read_dir(root.join("proj"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "mp4"))
            .count(),
        0,
        "no MP4 was written"
    );
}

#[test]
fn adapter_free_projects_print_no_readiness() {
    let root = tmp("plain");
    proj(
        &root,
        "composition name=\"main\" width=64 height=64 duration=1 background=\"#000\" version=1 {\n    text value=\"hi\" start=0 duration=1\n}\n"
            .as_bytes(),
    );
    let (code, text) = run(&["check", "proj"], &root);
    assert_eq!(code, 0, "plain project checks clean:\n{text}");
    assert!(
        !text.contains("readiness"),
        "adapter-free output is byte-identical:\n{text}"
    );
    let (code, text) = run(&["check", "--json", "proj"], &root);
    assert_eq!(code, 0, "plain JSON checks clean:\n{text}");
    assert!(
        text.contains("\"readiness\":[]"),
        "empty readiness stays machine-readable:\n{text}"
    );
}
