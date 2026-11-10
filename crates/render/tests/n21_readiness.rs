//! Sprint 21 T007: the prepare-time adapter gate through
//! [`render::readiness`] plus the loading block on custom sources.
//!
//! Covers the render half of `T007-report-readiness-and-lint.md`:
//! file-backed sources lost after validation refuse before the first
//! frame naming the file and cause; valid projects prepare exactly
//! as before; a still-loading custom source blocks its render by
//! name, and the registry readiness the inspector reads is the same
//! value the render path gates on.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-render-n21readiness-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn slide_json() -> Vec<u8> {
    serde_json::json!({
        "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 60.0, "w": 64.0, "h": 64.0,
        "nm": "slide",
        "markers": [{"tm": 0.0, "cm": "intro", "dr": 30.0}],
        "layers": [{
            "ddd": 0, "ind": 1, "ty": 4, "nm": "box", "sr": 1.0,
            "ks": {
                "o": {"a": 0, "k": 100.0},
                "r": {"a": 0, "k": 0.0},
                "p": {"a": 1, "k": [
                    {"t": 0.0, "s": [10.0, 32.0, 0.0], "e": [50.0, 32.0, 0.0]},
                    {"t": 30.0, "s": [50.0, 32.0, 0.0]}
                ]},
                "a": {"a": 0, "k": [0.0, 0.0, 0.0]},
                "s": {"a": 0, "k": [100.0, 100.0, 100.0]}
            },
            "shapes": [{
                "ty": "gr", "it": [
                    {"ty": "rc", "p": {"a": 0, "k": [0.0, 0.0]}, "s": {"a": 0, "k": [20.0, 20.0]}, "r": {"a": 0, "k": 0.0}},
                    {"ty": "fl", "c": {"a": 0, "k": [1.0, 0.0, 0.0, 1.0]}, "o": {"a": 0, "k": 100.0}},
                    {"ty": "tr", "p": {"a": 0, "k": [0.0, 0.0]}, "a": {"a": 0, "k": [0.0, 0.0]},
                     "s": {"a": 0, "k": [100.0, 100.0]}, "r": {"a": 0, "k": 0.0}, "o": {"a": 0, "k": 100.0}}
                ]
            }]
        }]
    })
    .to_string()
    .into_bytes()
}

fn valid_lottie_project(dir: &Path) {
    std::fs::write(dir.join("slide.json"), slide_json()).unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=64 height=64 duration=2 fps=30 background=\"#000000\" version=1 {\n    lottie path=\"slide.json\" start=0 width=64 height=64\n}\n",
    )
    .unwrap();
}

fn validated(dir: &Path) -> notation::model::Composition {
    let v = validate::validate::validate_folder(dir);
    assert_eq!(
        v.status,
        validate::validate::Status::Valid,
        "{:?}",
        v.diagnostics
    );
    v.composition.unwrap()
}

#[test]
fn intact_sources_prepare_exactly_as_before() {
    let dir = tmp("intact");
    valid_lottie_project(&dir);
    let comp = validated(&dir);
    // The gate passes silently: one metadata probe, no behavior change.
    assert!(render::readiness::gate_adapter_sources(&dir, &[&comp]).is_ok());
    assert!(render::readiness::vanished_sources(&dir, &[&comp]).is_empty());
    let mut prep =
        render::render::prepare_frames(&comp, &[comp.clone()], &dir, Path::new("ffmpeg"))
            .expect("intact lottie prepares");
    let first = prep.cpu_frame(0).expect("frame 0 draws");
    // Reruns stay byte-identical: the gate decodes nothing.
    let mut prep2 =
        render::render::prepare_frames(&comp, &[comp.clone()], &dir, Path::new("ffmpeg"))
            .expect("rerun prepares");
    assert_eq!(prep2.cpu_frame(0).expect("rerun draws"), first);
}

#[test]
fn source_lost_after_check_refuses_before_the_first_frame() {
    let dir = tmp("vanished");
    valid_lottie_project(&dir);
    let comp = validated(&dir);
    // The file vanishes between `check` and `render`.
    std::fs::remove_file(dir.join("slide.json")).unwrap();
    // Revalidation agrees with the render gate: the same source
    // lists failed with the file validation refused with.
    let rechecked = validate::validate::validate_folder(&dir);
    assert_eq!(rechecked.readiness.len(), 1);
    assert_eq!(rechecked.readiness[0].source, "slide.json");
    assert!(!rechecked.readiness[0].state.is_ready());
    // The inspector listing names the lost source, once per source.
    let vanished = render::readiness::vanished_sources(&dir, &[&comp]);
    assert_eq!(vanished.len(), 1);
    assert_eq!(
        (vanished[0].kind, vanished[0].source.as_str()),
        ("lottie", "slide.json")
    );
    // The gate refuses naming the file and cause, before any frame.
    let err = render::readiness::gate_adapter_sources(&dir, &[&comp]).expect_err("gate refuses");
    assert!(err.contains("slide.json"), "names the file: {err}");
    assert!(err.contains("no MP4 was written"), "writes nothing: {err}");
    // Preparation refuses the same way: no partial MP4.
    let err =
        match render::render::prepare_frames(&comp, &[comp.clone()], &dir, Path::new("ffmpeg")) {
            Ok(_) => panic!("prepare refuses a vanished source"),
            Err(e) => e,
        };
    assert!(err.contains("slide.json"), "prepare names the file: {err}");
}

#[test]
fn lost_chart_data_file_refuses_naming_the_file() {
    let dir = tmp("chart-data");
    std::fs::write(dir.join("sales.csv"), "Q1,12\nQ2,19\n").unwrap();
    std::fs::write(
        dir.join("composition.kdl"),
        "composition name=\"main\" width=640 height=480 duration=2 background=\"#000\" version=1 {\n    chart kind=\"bar\" data=\"sales.csv\" start=0 duration=2\n}\n",
    )
    .unwrap();
    let comp = validated(&dir);
    std::fs::remove_file(dir.join("sales.csv")).unwrap();
    let err = render::readiness::gate_adapter_sources(&dir, &[&comp]).expect_err("gate refuses");
    assert!(err.contains("sales.csv"), "names the data file: {err}");
    assert!(err.contains("chart"), "names the kind: {err}");
}

#[test]
fn loading_custom_source_blocks_its_render_by_name() {
    use render::custom_source::{CustomSourceRegistry, Readiness};
    let mut registry = CustomSourceRegistry::new();
    registry
        .register("hero", 0.0, 4.0, 64, 64, vec![0u8; 8])
        .expect("registration starts loading");
    // The inspector reads `loading`: the same value the render path
    // gates on below.
    assert_eq!(registry.readiness("hero").unwrap(), Readiness::Loading);
    // The render refuses by name: no pixels, no partial frame.
    let err = registry
        .render_at("hero", 1.0)
        .expect_err("loading blocks the render");
    assert!(
        format!("{err:?}").contains("hero"),
        "refusal names the source: {err:?}"
    );
    // A failed source names its cause instead.
    registry
        .mark_failed("hero", "pack bytes are not a zip")
        .unwrap();
    assert_eq!(
        registry.readiness("hero").unwrap(),
        Readiness::Failed {
            cause: "pack bytes are not a zip".to_string()
        }
    );
    let err = registry
        .render_at("hero", 1.0)
        .expect_err("failed names the cause");
    let text = format!("{err:?}");
    assert!(text.contains("hero"), "names the source: {text}");
    assert!(text.contains("not a zip"), "names the cause: {text}");
    // Ready sources render: the block lifts exactly on readiness.
    registry.mark_ready("hero").unwrap();
    assert_eq!(registry.readiness("hero").unwrap(), Readiness::Ready);
    let first = registry.render_at("hero", 1.0).expect("ready renders");
    assert_eq!(
        registry.render_at("hero", 1.0).expect("ready rerenders"),
        first,
        "ready reruns are byte-identical"
    );
}
