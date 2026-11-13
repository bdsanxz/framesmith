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

/// Stage a single-index project from raw HTML.
fn stage_one(tag: &str, html: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = tmp(tag);
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("index.html"), html).unwrap();
    let out = root.join("out");
    (root, src, out)
}

#[test]
fn convert_tuples_arrays_and_arithmetic_positions() {
    let (_root, src, out) = stage_one(
        "tuples",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"8\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"b\" style=\"left:200px;top:10px;width:100px;height:50px;background:#00ff00\"></div>\n<div id=\"c\" style=\"left:10px;top:200px;width:100px;height:50px;background:#0000ff\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\nconst D = 1.0;\nconst P = D + 0.5;\ntl.to(\"#a\", { x: 50, duration: 0.5, ease: \"power1.out\" }, P);\ntl.to(\"#b\", { x: 50, duration: 0.5, ease: \"power1.out\" }, D * 2 + 0.1,);\ntl.to([\"#a\", \"#b\"], { opacity: 0.5, duration: 0.2, ease: \"none\" }, 1.2);\ntl.to(\"#c\", { x: 10, ease: \"none\" }, 0.3, 2.2);\ntl.fromTo(\"#a\", { opacity: 0 }, { opacity: 1, duration: 9, ease: \"none\" }, 0.4, 2.6);\ntl.fromTo([\"#b\", \"#c\"], { scale: 0.5 }, { scale: 1, duration: 0.4, ease: \"none\" }, 3.0);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "tuples/arrays/arithmetic must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // Const-chain position P = D + 0.5 lands at 1.5.
    assert!(kdl.contains("start=1.5"), "P position: {kdl}");
    // Trailing-comma arithmetic D * 2 + 0.1 lands at 2.1.
    assert!(kdl.contains("start=2.1"), "arith position: {kdl}");
    // Array targets emit one line per entry at the same start.
    let array_hits = kdl.matches("start=1.2").count();
    assert_eq!(array_hits, 2, "array fan-out: {kdl}");
    // Positional (targets, vars, duration, position) form.
    assert!(
        kdl.contains("start=2.2 duration=0.3"),
        "positional duration: {kdl}"
    );
    // Positional (targets, from, to, duration, position) overrides vars.
    assert!(
        kdl.contains("start=2.6 duration=0.4"),
        "fromTo positional duration: {kdl}"
    );
    // Array fromTo at 3.0 emits twice.
    let arr2 = kdl.matches("start=3 duration=0.4").count();
    assert_eq!(arr2, 2, "array fromTo: {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "no refusals: {fidelity}");
    assert_no_silent_approximation(&fidelity);
    // The override is named as an approximation with its delta.
    let entries = v["entries"].as_array().unwrap();
    let over = entries
        .iter()
        .find(|e| e["construct"] == "tl.fromTo" && e["detail"].as_str().unwrap_or("").contains("start=2.6000"))
        .expect("fromTo override verdict");
    assert_eq!(over["verdict"], "approximated", "{fidelity}");
    assert!(
        over["delta"].as_str().unwrap_or("").contains("overrode vars duration"),
        "delta: {over}"
    );
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
    // Re-conversion of the new forms is byte-identical.
    let out2 = _root.join("out2");
    let (code2, _) = run(
        &["convert", src.to_str().unwrap(), "--out", out2.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code2, 0);
    assert_eq!(
        std::fs::read(out.join("composition.kdl")).unwrap(),
        std::fs::read(out2.join("composition.kdl")).unwrap()
    );
}

#[test]
fn convert_typeon_macro_expands_per_glyph() {
    let (_root, src, out) = stage_one(
        "typeon",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"8\" data-width=\"640\" data-height=\"360\">\n<div id=\"msg\" style=\"left:10px;top:10px;width:200px;height:50px\">Hello</div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\nfunction typeOn(sel, t, cps) {\n  const el = document.querySelector(sel);\n  const text = el.textContent;\n  el.textContent = \"\";\n  [...text].forEach((ch, i) => {\n    const s = document.createElement(\"span\");\n    s.textContent = ch;\n    el.appendChild(s);\n    tl.fromTo(s, { opacity: 0 }, { opacity: 1, duration: 0.001, immediateRender: true }, t + i / cps);\n  });\n}\ntypeOn(\"#msg\", 1.0, 2);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "typeOn macro must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // One from-to opacity tween per glyph at t + i / cps.
    for needle in ["start=1 ", "start=1.5 ", "start=2 ", "start=2.5 ", "start=3 "] {
        assert!(kdl.contains(needle), "glyph line {needle:?}: {kdl}");
    }
    let glyphs = kdl.matches("kind=\"from-to\"").count();
    assert_eq!(glyphs, 5, "five per-glyph lines for Hello: {kdl}");
    // The typewriter render path is preserved.
    assert!(kdl.contains("split=\"chars\""), "split: {kdl}");
    assert!(kdl.contains("typewriter=true"), "typewriter: {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "no refusals: {fidelity}");
    let entries = v["entries"].as_array().unwrap();
    let t = entries
        .iter()
        .find(|e| e["construct"] == "typeOn")
        .expect("typeOn verdict");
    assert_eq!(t["verdict"], "approximated", "{fidelity}");
    assert!(
        t["delta"].as_str().unwrap_or("").contains("clamped"),
        "delta names the clamp: {t}"
    );
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

#[test]
fn convert_typeon_set_display_template_expands_per_glyph() {
    let (_root, src, out) = stage_one(
        "typeon-set",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"8\" data-width=\"640\" data-height=\"360\">\n<div id=\"msg\" style=\"left:10px;top:10px;width:200px;height:50px\">Hi!</div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\nfunction typeOn(sel, t, cps) {\n  const el = document.querySelector(sel);\n  const text = el.textContent;\n  el.textContent = \"\";\n  [...text].forEach((ch, i) => {\n    const s = document.createElement(\"span\");\n    s.textContent = ch;\n    s.style.display = \"none\";\n    el.appendChild(s);\n    tl.set(s, { display: \"inline\" }, t + i / cps);\n  });\n}\ntypeOn(\"#msg\", 1.0, 2);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "set-display typeOn must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    for needle in ["start=1 ", "start=1.5 ", "start=2 "] {
        assert!(kdl.contains(needle), "glyph line {needle:?}: {kdl}");
    }
    assert_eq!(
        kdl.matches("property=\"opacity\" start=").count(),
        3,
        "three per-glyph sets: {kdl}"
    );
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "no refusals: {fidelity}");
    let entries = v["entries"].as_array().unwrap();
    let t = entries
        .iter()
        .find(|e| e["construct"] == "typeOn")
        .expect("typeOn verdict");
    assert_eq!(t["verdict"], "approximated", "{fidelity}");
    assert!(
        t["delta"].as_str().unwrap_or("").contains("display"),
        "delta names the display substitution: {t}"
    );
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

/// Convert a one-call fixture; assert refusal naming the call site.
fn assert_refuses(tag: &str, script: &str, needle: &str) {
    let (_root, src, out) = stage_one(
        tag,
        &format!(
            "<html><head><meta charset=\"UTF-8\" /><style>#root {{ background: #000000; }}</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"2\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"t\" style=\"left:10px;top:100px;width:100px;height:50px\">Hi</div>\n</div><script>\nconst tl = gsap.timeline({{ paused: true }});\n{script}\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n"
        ),
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 1, "must refuse: {text}");
    assert!(!out.join("composition.kdl").exists(), "no KDL on refusal");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    assert!(fidelity.contains("\"verdict\": \"refused\""), "{fidelity}");
    assert!(fidelity.contains(needle), "names {needle:?}: {fidelity}");
}

#[test]
fn convert_data_uri_assets_decode_to_files() {
    let (_root, src, out) = stage_one(
        "datauri",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<img id=\"pic\" src=\"data:image/svg+xml,%3Csvg xmlns%3D'http://www.w3.org/2000/svg' width%3D'64' height%3D'64'%3E%3Crect width%3D'64' height%3D'64' fill%3D'%23ff0000'/%3E%3C/svg%3E\" style=\"left:10px;top:10px;width:64px;height:64px\" />\n<img id=\"dot\" src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8Xw8AAwMCAO+ip1sAAAAASUVORK5CYII=\" style=\"left:100px;top:10px;width:8px;height:8px\" />\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#pic\", { opacity: 0 }, { opacity: 1, duration: 0.5, ease: \"power1.out\" }, 0.2);\ntl.fromTo(\"#dot\", { opacity: 0 }, { opacity: 1, duration: 0.5, ease: \"power1.out\" }, 0.4);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "data: assets must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    assert!(
        kdl.contains("path=\"assets/data-pic.svg\""),
        "svg data uri landed: {kdl}"
    );
    // Inline style geometry (left/top/width/height in any order).
    assert!(kdl.contains("x=10 y=10 width=64 height=64"), "inline style geometry: {kdl}");
    assert!(
        kdl.contains("path=\"assets/data-dot.png\""),
        "base64 png landed: {kdl}"
    );
    // The decoded bytes are real files in the output assets tree.
    let svg = std::fs::read(out.join("assets/data-pic.svg")).unwrap();
    assert_eq!(
        svg,
        b"<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='#ff0000'/></svg>"
    );
    let png = std::fs::read(out.join("assets/data-dot.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "png magic on disk");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    let asset_entries = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["construct"] == "asset img")
        .collect::<Vec<_>>();
    assert_eq!(asset_entries.len(), 2, "two data assets: {fidelity}");
    assert_eq!(asset_entries[0]["verdict"], "translated", "{fidelity}");
    assert!(
        asset_entries[0]["detail"].as_str().unwrap().contains("percent-encoded"),
        "{asset_entries:?}"
    );
    assert!(
        asset_entries[1]["detail"].as_str().unwrap().contains("base64"),
        "{asset_entries:?}"
    );
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
    // Byte-identical re-conversion, files included.
    let out2 = _root.join("out2");
    let (code2, _) = run(
        &["convert", src.to_str().unwrap(), "--out", out2.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code2, 0);
    assert_eq!(
        std::fs::read(out.join("composition.kdl")).unwrap(),
        std::fs::read(out2.join("composition.kdl")).unwrap()
    );
    assert_eq!(
        std::fs::read(out.join("assets/data-pic.svg")).unwrap(),
        std::fs::read(out2.join("assets/data-pic.svg")).unwrap()
    );
}

#[test]
fn convert_data_uri_unsupported_mime_refuses() {
    let (_root, src, out) = stage_one(
        "datauri-mime",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"2\" data-width=\"640\" data-height=\"360\">\n<img id=\"pic\" src=\"data:image/webp;base64,UklGRiQAAABXRUJQVlA4WAoAAAAQAAAABwAAcQkRZm9v\" style=\"left:10px;top:10px;width:64px;height:64px\" />\n</div><script>const tl = gsap.timeline({ paused: true }); window.__timelines[\"main\"] = tl;</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 1, "unsupported mime must refuse: {text}");
    assert!(text.contains("image/webp"), "names the mime: {text}");
    assert!(!out.join("composition.kdl").exists(), "no KDL on refusal");
}

#[test]
fn convert_immediate_render_approximates_with_delta() {
    let (_root, src, out) = stage_one(
        "immrender",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#a\", { opacity: 0 }, { opacity: 1, duration: 0.5, ease: \"power1.out\", immediateRender: false }, 0.3);\ntl.to(\"#a\", { x: 20, duration: 0.4, ease: \"none\", immediateRender: true }, 1.0);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "immediateRender approximates, not refuses: {text}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    let imm = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["construct"] == "tl.fromTo" || e["construct"] == "tl.to")
        .collect::<Vec<_>>();
    assert_eq!(imm.len(), 2, "both calls landed: {fidelity}");
    for e in &imm {
        assert_eq!(e["verdict"], "approximated", "{fidelity}");
        let delta = e["delta"].as_str().unwrap_or("");
        assert!(
            delta.contains("immediateRender") && delta.contains("carryback"),
            "delta names the approximation: {e}"
        );
    }
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

#[test]
fn convert_filter_chain_lands_blur_motion() {
    let (_root, src, out) = stage_one(
        "filterchain",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"b\" style=\"left:200px;top:10px;width:100px;height:50px;background:#00ff00\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#a\", { opacity: 0, filter: \"blur(6px)\" }, { opacity: 1, filter: \"blur(0px)\", duration: 0.9, ease: \"power2.out\" }, 0);\ntl.fromTo(\"#b\", { filter: \"grayscale(1) blur(4px) brightness(0.36)\" }, { filter: \"grayscale(0) blur(0px) brightness(1)\", duration: 0.5, ease: \"none\" }, 0.2);\ngsap.set(\"#a\", { filter: \"blur(8px)\" });\ntl.to(\"#a\", { filter: \"blur(0px)\", duration: 0.5, ease: \"none\" }, 1.0);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "filter chains must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // fromTo blur pair lands exactly.
    assert!(
        kdl.contains("property=\"blur\" kind=\"from-to\" start=0 duration=0.9 from=6 to=0"),
        "blur from-to: {kdl}"
    );
    // The remainder chain's blur lands too.
    assert!(
        kdl.contains("from=4 to=0"),
        "remainder chain blur: {kdl}"
    );
    // set + to on the same (clip, property): the set value is
    // captured as an exact from-to, and the set hold truncates.
    assert!(
        kdl.contains("from=8 to=0"),
        "set-then-to from-to: {kdl}"
    );
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    let chain = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["construct"] == "tl.fromTo" && e["detail"].as_str().unwrap_or("").contains("start=0.2000"))
        .expect("chain verdict");
    assert_eq!(chain["verdict"], "approximated", "{fidelity}");
    let delta = chain["delta"].as_str().unwrap_or("");
    assert!(
        delta.contains("grayscale(1)") && delta.contains("brightness(0.36)"),
        "delta names the remainder: {chain}"
    );
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

#[test]
fn convert_compound_selectors_resolve_to_clips() {
    let (_root, src, out) = stage_one(
        "compound",
        "<html><head><meta charset=\"UTF-8\" /><style>\n#root { background: #000000; }\n#pin { left: 400px; top: 200px; width: 100px; height: 100px; }\n#pin svg { left: -14px; top: -40px; width: 28px; height: 40px; }\n#box { left: 100px; top: 100px; width: 100px; height: 100px; }\n</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"pin\"><svg viewBox=\"0 0 28 40\"><path d=\"M14 39 C9 29 1 23 1 14 A13 13 0 0 1 27 14 C27 23 19 29 14 39 Z\" fill=\"#ff8c42\" /></svg></div>\n<div id=\"box\"><span class=\"mark\">Hey</span></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#pin svg\", { y: -50, opacity: 0 }, { y: 0, opacity: 1, duration: 0.3, ease: \"power1.out\" }, 0.5);\ntl.fromTo(\"#box .mark\", { opacity: 0 }, { opacity: 1, duration: 0.4, ease: \"none\" }, 1.0);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "compound selectors must resolve: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // Inline svg extracted beside the conversion, at the parent +
    // compound offset (400 - 14, 200 - 40).
    assert!(
        kdl.contains("path=\"assets/data-index-"),
        "extracted svg clip: {kdl}"
    );
    assert!(kdl.contains("x=386 y=160"), "svg parent offset: {kdl}");
    assert!(kdl.contains("width=28 height=40"), "svg compound size: {kdl}");
    // Motion lands on both resolved clips.
    assert!(
        kdl.contains("start=0.5 duration=0.3") && kdl.contains("start=1 duration=0.4"),
        "compound motion: {kdl}"
    );
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    let svg_entry = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["construct"] == "asset svg")
        .expect("svg extraction verdict");
    assert_eq!(svg_entry["verdict"], "translated", "{fidelity}");
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

#[test]
fn convert_filter_unknown_function_refuses_naming_it() {
    assert_refuses(
        "filter-unknown",
        "tl.to(\"#a\", { filter: \"hue-rotate(90deg)\", duration: 0.3 }, 0);",
        "hue-rotate",
    );
}

#[test]
fn convert_outside_grammar_refuses_naming_site() {
    // A loop index is inside the arithmetic grammar but unbound
    // outside the typeOn macro; the refusal names it.
    assert_refuses(
        "loop-idx",
        "tl.to(\"#a\", { x: 5, duration: 0.3 }, 0.02 * i);",
        "unbound identifier",
    );
    // Member access is outside the grammar entirely.
    assert_refuses(
        "member",
        "tl.to(\"#a\", { x: 5, duration: 0.3 }, g.start);",
        "g.start",
    );
    // Array entries must each resolve to a converted clip.
    assert_refuses(
        "arr-unknown",
        "tl.to([\"#a\", \"#nope\"], { x: 5, duration: 0.3 }, 0.1);",
        "names no converted clip",
    );
    // A typeOn definition that does not match the per-glyph shape
    // refuses its call sites.
    assert_refuses(
        "typeon-shape",
        "function typeOn(sel, t, cps) {\n  tl.fromTo(document.querySelector(sel), { opacity: 0 }, { opacity: 1, duration: 0.5 }, t + i / cps);\n}\ntypeOn(\"#t\", 1.0, 2);",
        "per-glyph fromTo grammar",
    );
}

/// Sprint 28 lane (transforms/keyframes/easings): `steps(N)` and
/// `back.out(s)` land on their exact KDL curves with no delta.
#[test]
fn convert_steps_and_back_out_land_on_exact_curves() {
    let (_root, src, out) = stage_one(
        "stepsease",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"b\" style=\"left:200px;top:10px;width:100px;height:50px;background:#00ff00\"></div>\n<div id=\"c\" style=\"left:400px;top:10px;width:100px;height:50px;background:#0000ff\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#a\", { scale: 0 }, { scale: 1, duration: 0.2, ease: \"steps(4)\" }, 0.05);\ntl.to(\"#b\", { x: 100, duration: 0.5, ease: \"back.out(2)\" }, 0.5);\ntl.to(\"#c\", { x: 100, duration: 0.5, ease: \"back.out(2.5)\" }, 0.5);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "steps/back.out must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    assert!(kdl.contains("ease=\"steps(4)\""), "steps: {kdl}");
    assert!(kdl.contains("ease=\"out-back(2)\""), "back.out(2): {kdl}");
    assert!(kdl.contains("ease=\"out-back(2.5)\""), "back.out(2.5): {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    // The three ease calls land translated (the fps entry is the only
    // approximation in this fixture).
    let ease_calls = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["construct"] == "tl.fromTo" || e["construct"] == "tl.to")
        .collect::<Vec<_>>();
    assert_eq!(ease_calls.len(), 3, "{fidelity}");
    for e in &ease_calls {
        assert_eq!(e["verdict"], "translated", "{fidelity}");
    }
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

/// Sprint 28 lane: `transformOrigin` snaps onto the clip `origin`
/// pivot with the snap error and the 2D-pivot caveat named.
#[test]
fn convert_transform_origin_snaps_with_delta() {
    let (_root, src, out) = stage_one(
        "origin",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"b\" style=\"left:200px;top:10px;width:100px;height:50px;background:#00ff00\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#a\", { scale: 0 }, { scale: 1, transformOrigin: \"0 50%\", duration: 0.5 }, 0);\ntl.to(\"#b\", { scale: 2, transformOrigin: \"90% 10%\", duration: 0.5 }, 0.2);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "transformOrigin must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // "0 50%" snaps exactly to the left-center pivot; "90% 10%" snaps
    // to right-top (off by 0.10 x / 0.10 y).
    assert!(kdl.contains("origin=\"left center\""), "snap: {kdl}");
    assert!(kdl.contains("origin=\"right top\""), "snap: {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    let delta = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["verdict"] == "approximated")
        .map(|e| e["delta"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("; ");
    assert!(delta.contains("transformOrigin"), "delta names origin: {delta}");
    assert!(delta.contains("pivot off by 0.100"), "delta measures snap: {delta}");
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
}

/// Sprint 28 lane: a pixel `transformOrigin` without a known
/// element size lands approximated with the loss named (the rest of
/// the call still converts); unreadable origins refuse.
#[test]
fn convert_pixel_origin_unknown_size_approximates() {
    let (_root, src, out) = stage_one(
        "origin-px",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"4\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" class=\"cam\"><div class=\"sheet\" style=\"width:2200px;height:1240px\"></div></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.set(\"#a\", { transformOrigin: \"731.131px 507.6px\" }, 0);\ntl.to(\"#a\", { scale: 2, duration: 0.5 }, 0.2);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "pixel origin must approximate: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    assert!(!kdl.contains("origin="), "no pivot written: {kdl}");
    assert!(kdl.contains("property=\"scale\""), "scale lands: {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    assert!(fidelity.contains("731.131px"), "delta names origin: {fidelity}");
    assert!(fidelity.contains("element size is unknown"), "{fidelity}");
    assert_no_silent_approximation(&fidelity);
    // An unreadable origin refuses naming the value.
    assert_refuses(
        "origin-bad",
        "tl.to(\"#a\", { scale: 2, transformOrigin: \"middle-earth\", duration: 0.5 }, 0);",
        "transformOrigin",
    );
}

/// Sprint 28 lane: GSAP keyframes arrays land as KDL `key` blocks.
#[test]
fn convert_keyframes_land_as_key_blocks() {
    let (_root, src, out) = stage_one(
        "keyframes",
        "<html><head><meta charset=\"UTF-8\" /><style>#root { background: #000000; }</style></head>\n<body><div id=\"root\" data-composition-id=\"main\" data-duration=\"6\" data-width=\"640\" data-height=\"360\">\n<div id=\"a\" style=\"left:10px;top:10px;width:100px;height:50px;background:#ff0000\"></div>\n<div id=\"b\" style=\"left:200px;top:10px;width:100px;height:50px;background:#00ff00\"></div>\n</div><script>\nconst tl = gsap.timeline({ paused: true });\ntl.fromTo(\"#a\", { scale: 0 }, { keyframes: [{ scale: 1.25, duration: 0.12 }, { scale: 0.9, duration: 0.08 }, { scale: 1, duration: 0.08 }], ease: \"steps(4)\" }, 0.55);\ntl.to(\"#b\", { keyframes: [{ opacity: 0.55, duration: 0.12, ease: \"power1.in\" }, { opacity: 1, duration: 0.1, ease: \"power1.out\" }] }, 1.0);\nwindow.__timelines[\"main\"] = tl;\n</script></body></html>\n",
    );
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "keyframes must convert: {text}");
    let kdl = std::fs::read_to_string(out.join("composition.kdl")).unwrap();
    // fromTo capture: first frame at the start with the from value.
    assert!(kdl.contains("key target=0 property=\"scale\""), "key block: {kdl}");
    assert!(kdl.contains("frame at=0.55 value=0"), "capture frame: {kdl}");
    assert!(kdl.contains("frame at=0.67 value=1.25"), "stop 1: {kdl}");
    assert!(kdl.contains("frame at=0.75 value=0.9"), "stop 2: {kdl}");
    assert!(kdl.contains("frame at=0.83 value=1"), "stop 3: {kdl}");
    // The call ease rides every segment (fallback for easing-less stops).
    assert!(kdl.contains("ease=\"steps(4)\""), "parent ease: {kdl}");
    // Per-stop eases ride their segments on the `to` chain.
    assert!(kdl.contains("ease=\"in-quad\""), "stop ease: {kdl}");
    assert!(kdl.contains("ease=\"out-quad\""), "stop ease: {kdl}");
    let fidelity = std::fs::read_to_string(out.join("fidelity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&fidelity).unwrap();
    assert_eq!(v["counts"]["refused"], 0, "{fidelity}");
    assert_no_silent_approximation(&fidelity);
    let (code, text) = run(&["check", out.to_str().unwrap()], Path::new("."));
    assert_eq!(code, 0, "converted project must check clean: {text}");
    // Determinism: rerun the conversion and compare bytes.
    let out2 = out.join("again");
    let (code, text) = run(
        &["convert", src.to_str().unwrap(), "--out", out2.to_str().unwrap()],
        Path::new("."),
    );
    assert_eq!(code, 0, "rerun must convert: {text}");
    assert_eq!(
        std::fs::read(out.join("composition.kdl")).unwrap(),
        std::fs::read(out2.join("composition.kdl")).unwrap(),
        "KDL must be byte-identical"
    );
    assert_eq!(
        std::fs::read(out.join("fidelity.json")).unwrap(),
        std::fs::read(out2.join("fidelity.json")).unwrap(),
        "fidelity must be byte-identical"
    );
}

/// Sprint 28 lane refusals: keyframes outside the grammar and
/// `strokeDashoffset` (no KDL motion property) name their cause.
#[test]
fn convert_keyframes_and_stroke_dash_refuse_naming() {
    // Keyframes as an identifier (array variable).
    assert_refuses(
        "kf-ident",
        "const path = [ { scale: 1, duration: 0.3 } ];\ntl.fromTo(\"#a\", { scale: 0 }, { keyframes: path }, 0);",
        "not an array literal",
    );
    // A stop property with no KDL motion property.
    assert_refuses(
        "kf-badprop",
        "tl.to(\"#a\", { keyframes: [{ v: 1, duration: 0.3 }, { v: 2, duration: 0.3 }] }, 0);",
        "has no KDL motion property",
    );
    // `set` takes no chained form.
    assert_refuses(
        "kf-set",
        "gsap.set(\"#a\", { keyframes: [{ scale: 1, duration: 0.3 }, { scale: 2, duration: 0.3 }] });",
        "on a `set` call",
    );
    // strokeDashoffset has no KDL motion property (loading bars use CSS
    // stroke transitions; there is no dash motion property to invent).
    assert_refuses(
        "stroke-dash",
        "tl.to(\"#a\", { strokeDashoffset: 0, duration: 0.5 }, 0);",
        "strokeDashoffset",
    );
}
