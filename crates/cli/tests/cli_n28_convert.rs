//! Sprint 28 T001: the HTML→KDL converter (`framesmith convert`).
//!
//! Targeted proofs (no full-suite run required by the task): one
//! reference converts into a Framesmith project that renders without
//! obstacles (`check` exits 0), GSAP starts/durations land verbatim,
//! every approximation names measured deltas (no silent approximation
//! passes the report), and parse/asset/support refusals name their
//! causes (file, call site, asset).
//!
//! Fixtures are self-contained temp projects; the binary is invoked by
//! path (`CARGO_BIN_EXE_framesmith`), never by is_file() PATH joins.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-n28-convert-{}-{}-{}",
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

/// A minimal HyperFrames-shaped project: root + one sub-composition
/// with five `tl.fromTo` calls (same shape as name-card-template).
const INDEX: &str = "<!doctype html>\n<!--\n  GUIDE: index.html (root composition, id \"main\", 5 seconds, 1920 by 1080)\n  WHAT: a tiny card.\n-->\n<html data-composition-variables='[{\"id\":\"accent\",\"type\":\"color\",\"label\":\"Accent\",\"default\":\"#7c5cff\"}]'>\n<head><meta charset=\"UTF-8\" /><style>\nhtml, body { width: 1920px; height: 1080px; background: #0e0f14; }\n</style></head>\n<body>\n<div id=\"root\" data-composition-id=\"main\" data-start=\"0\" data-duration=\"5\" data-width=\"1920\" data-height=\"1080\">\n<div id=\"slot\" data-composition-id=\"card\" data-composition-src=\"compositions/card.html\" data-start=\"0\" data-duration=\"5\" data-track-index=\"0\" data-track-kind=\"graphics\"></div>\n</div>\n<script>\nconst tl = gsap.timeline({ paused: true });\nwindow.__timelines[\"main\"] = tl;\n</script>\n</body></html>\n";

const CARD: &str = "<html><body><template>\n<style>\n#card-bar { position: absolute; left: 200px; top: 380px; width: 16px; height: 320px; background: var(--accent, #7c5cff); }\n#card-text { position: absolute; left: 260px; top: 380px; display: flex; flex-direction: column; gap: 24px; }\n#card-hello { font-size: 34px; color: #a2a6b8; }\n#card-name { font-size: 150px; color: #f4f5fa; }\n#card-role { font-size: 48px; color: var(--accent, #7c5cff); }\n</style>\n<div id=\"root\" data-composition-id=\"card\" data-width=\"1920\" data-height=\"1080\">\n<span id=\"card-bar\"></span>\n<div id=\"card-text\">\n<div id=\"card-hello\">WELCOME</div>\n<div id=\"card-name\" data-var-text=\"name\">Ada Lovelace</div>\n<div id=\"card-role\" data-var-text=\"role\">First programmer</div>\n</div>\n</div>\n<script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#card-bar\", { scaleY: 0 }, { scaleY: 1, duration: 0.6, ease: \"power3.out\" }, 0.2);\ntl.fromTo(\"#card-hello\", { opacity: 0, x: -20 }, { opacity: 1, x: 0, duration: 0.5, ease: \"power3.out\" }, 0.4);\ntl.fromTo(\"#card-name\", { opacity: 0, y: 40 }, { opacity: 1, y: 0, duration: 0.7, ease: \"expo.out\" }, 0.55);\ntl.fromTo(\"#card-role\", { opacity: 0, y: 20 }, { opacity: 1, y: 0, duration: 0.5, ease: \"power3.out\" }, 0.9);\ntl.fromTo(\"#card-text\", { x: 0 }, { x: 24, duration: 4, ease: \"none\" }, 1.0);\nwindow.__timelines[\"card\"] = tl;\n</script>\n</template></body></html>\n";

fn stage_fixture(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = tmp(tag);
    let src = root.join("src");
    std::fs::create_dir_all(src.join("compositions")).unwrap();
    std::fs::write(src.join("index.html"), INDEX).unwrap();
    std::fs::write(src.join("compositions/card.html"), CARD).unwrap();
    std::fs::write(
        src.join("hyperframes.json"),
        "{\"paths\":{\"blocks\":\"compositions\",\"assets\":\"assets\"}}",
    )
    .unwrap();
    let out = root.join("out");
    (root, src, out)
}

/// Every approximated entry must carry a non-empty measured delta
/// (CAP-003: missing deltas fail the verdict).
fn assert_no_silent_approximation(fidelity: &str) {
    let v: serde_json::Value =
        serde_json::from_str(fidelity).expect("fidelity.json parses");
    let entries = v["entries"].as_array().expect("entries array");
    let refused = v["counts"]["refused"].as_u64().expect("counts");
    for e in entries {
        let verdict = e["verdict"].as_str().unwrap_or("");
        if verdict == "approximated" {
            let delta = e
                .get("delta")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            assert!(
                !delta.is_empty(),
                "approximated construct without delta: {}\n{}",
                e["construct"],
                fidelity
            );
        }
    }
    let _ = refused;
}

#[test]
fn convert_one_reference_renders_and_land_timings() {
    let (_root, src, out) = stage_fixture("happy");
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "convert must exit 0: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl"))
        .expect("composition.kdl written");
    let fidelity =
        std::fs::read_to_string(out.join("fidelity.json")).expect("fidelity.json written");
    // GSAP starts/durations/eases land verbatim (CAP-002).
    for needle in [
        "start=0.2 duration=0.6",
        "start=0.4 duration=0.5",
        "start=0.55 duration=0.7",
        "start=0.9 duration=0.5",
        "start=1 duration=4",
        "ease=\"out-quart\"",
        "ease=\"out-expo\"",
        "ease=\"linear\"",
    ] {
        assert!(kdl.contains(needle), "KDL missing {needle:?}: {kdl}");
    }
    // CSS-hyphenated style attrs only (durable rule).
    for camel in ["fontSize", "borderRadius", "letterSpacing", "backgroundColor"] {
        assert!(
            !kdl.contains(camel),
            "camelCase attr {camel:?} leaked into KDL: {kdl}"
        );
    }
    assert!(
        kdl.contains("property=\"scale\""),
        "hyphenated motion property expected: {kdl}"
    );
    // Flex children land stacked (260, 380/438/612).
    for needle in ["x=260 y=380", "x=260 y=438", "x=260 y=612"] {
        assert!(kdl.contains(needle), "KDL missing {needle:?}: {kdl}");
    }
    assert_no_silent_approximation(&fidelity);
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "no refusals: {fidelity}");
    assert_eq!(v["composition"], "main");
    // The converted project validates: renders without obstacles.
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
    assert!(text.contains("is valid"), "check verdict: {text}");
}

#[test]
fn convert_unparseable_refuses_naming_file() {
    let root = tmp("unparseable");
    // No index.html at all.
    let (code, text) = run(
        &["convert", root.to_str().unwrap(), "--out", root.join("out").to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 1, "unparseable must exit 1: {text}");
    assert!(text.contains("index.html"), "refusal names file: {text}");
    let fidelity =
        std::fs::read_to_string(root.join("out/fidelity.json")).expect("report written");
    assert!(fidelity.contains("\"verdict\": \"refused\""), "{fidelity}");
    assert!(
        !root.join("out/composition.kdl").exists(),
        "refusal writes no KDL"
    );
}

#[test]
fn convert_unsupported_call_refuses_naming_site() {
    let root = tmp("unsup");
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("index.html"),
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"2\" data-width=\"640\" data-height=\"360\">\n<div id=\"box\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.to(\"#box\", { x: 100, duration: 1, onUpdate: () => {} }, 0);\n</script></body></html>\n",
    )
    .unwrap();
    let out = root.join("out");
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 1, "unsupported call must exit 1: {text}");
    assert!(text.contains("tl.to"), "names the call: {text}");
    assert!(text.contains("onUpdate"), "names the cause: {text}");
    assert!(!out.join("composition.kdl").exists(), "no KDL on refusal");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    assert!(fidelity.contains("refused"), "{fidelity}");
}

#[test]
fn convert_missing_asset_refuses_naming_asset() {
    let root = tmp("missing-asset");
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("index.html"),
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"2\" data-width=\"640\" data-height=\"360\">\n<img id=\"hero\" src=\"assets/hero.png\" style=\"left:0px;top:0px;width:640px;height:360px\" />\n</div><script>const tl = gsap.timeline({ paused: true }); window.__timelines[\"main\"] = tl;</script></body></html>\n",
    )
    .unwrap();
    let out = root.join("out");
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 1, "missing asset must exit 1: {text}");
    assert!(text.contains("assets/hero.png"), "names the asset: {text}");
}

#[test]
fn convert_approximation_names_deltas() {
    let root = tmp("approx");
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("index.html"),
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"2\" data-width=\"640\" data-height=\"360\">\n<div id=\"box\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.to(\"#box\", { x: 100, duration: 1, ease: \"elastic.out\" }, 0);\n</script></body></html>\n",
    )
    .unwrap();
    let out = root.join("out");
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "elastic approximates, not refuses: {text}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    let entries = v["entries"].as_array().unwrap();
    let elastic = entries
        .iter()
        .find(|e| e["construct"] == "tl.to")
        .expect("tl.to verdict");
    assert_eq!(elastic["verdict"], "approximated", "{fidelity}");
    let delta = elastic["delta"].as_str().unwrap_or("");
    assert!(
        delta.contains("amplitude"),
        "delta names the measured loss: {delta}"
    );
    assert_no_silent_approximation(&fidelity);
}

#[test]
fn convert_is_byte_deterministic() {
    let (_root, src, out1) = stage_fixture("det1");
    let out2 = _root.join("out2");
    let (code1, _) = run(
        &["convert", src.to_str().unwrap(), "--out", out1.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code1, 0);
    let (code2, _) = run(
        &["convert", src.to_str().unwrap(), "--out", out2.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code2, 0);
    let kdl1 = std::fs::read(out1.join("composition.kdl")).unwrap();
    let kdl2 = std::fs::read(out2.join("composition.kdl")).unwrap();
    assert_eq!(kdl1, kdl2, "KDL must be byte-identical across runs");
    let f1 = std::fs::read(out1.join("fidelity.json")).unwrap();
    let f2 = std::fs::read(out2.join("fidelity.json")).unwrap();
    assert_eq!(f1, f2, "fidelity must be byte-identical across runs");
}
