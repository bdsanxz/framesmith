//! Inspector row regressions (Sprint 17, T007 / CAP-007, J-04,
//! REC-001): every row family applies through a validated patch turn
//! with the prior value restored by undo, per-row reset restores the
//! default as its own undoable turn, render-deferred rows refuse
//! naming their milestone, and every refusal names its rule and
//! location while the composition and the project tree stay exactly
//! as they were.

use sdk::{ClipRef, EditOp, OpenOptions, SdkSession};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-sdk-rows-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(folder: &Path, name: &str, content: &str) {
    std::fs::write(folder.join(name), content).unwrap();
}

fn project(folder: &Path) {
    std::fs::create_dir_all(folder.join("media")).unwrap();
    std::fs::write(folder.join("media/clip.mp4"), b"not-a-real-video").unwrap();
    std::fs::write(folder.join("media/sound.wav"), b"not-a-real-sound").unwrap();
    write(
        folder,
        "composition.kdl",
        "composition name=\"t\" width=640 height=360 duration=6 fps=10 background=\"#112233\" version=1 {\n    text id=\"title\" value=\"Hello\" start=0 duration=6 x=10 y=40 size=24 color=\"#ffffff\"\n    box id=\"card\" x=0 y=0 width=200 height=100 fill=\"#ff0000\" start=0 duration=6 {\n        effect preset=\"grayscale\"\n    }\n    video id=\"clip\" path=\"media/clip.mp4\" start=0 duration=4\n    audio id=\"music\" path=\"media/sound.wav\" start=0 duration=6\n}\n",
    );
}

fn open(folder: &Path) -> SdkSession {
    SdkSession::open(
        folder,
        OpenOptions {
            history: false,
            ..OpenOptions::default()
        },
    )
    .expect("valid open")
}

fn tree_bytes(folder: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .strip_prefix(folder)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.insert(key, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

fn row(folder: &Path, target: ClipRef, property: &str, value: &str) -> Result<String, String> {
    let mut session = open(folder);
    let op = EditOp::SetRow {
        target: ClipRef {
            id: target.id.clone(),
            index: target.index,
        },
        property: property.to_string(),
        value: value.to_string(),
    };
    match session.apply(op) {
        Ok(applied) => {
            let canon = sdk::canonical_row_property(property);
            let comp = session.effective_composition();
            let at = match (&target.id, target.index) {
                (Some(id), _) => comp.clips.iter().position(|c| c.id() == Some(id.as_str())),
                (None, Some(i)) => Some(i),
                (None, None) => None,
            };
            let live = at
                .and_then(|i| comp.clips.get(i))
                .and_then(|c| sdk::row_live_value(c, &canon))
                .unwrap_or_default();
            session.persist().map_err(|e| e.to_string())?;
            Ok(format!("{} => {live}", applied.op.summary()))
        }
        Err(e) => Err(e.to_string()),
    }
}

fn title() -> ClipRef {
    ClipRef::by_id("title")
}

fn card() -> ClipRef {
    ClipRef::by_id("card")
}

fn clip() -> ClipRef {
    ClipRef::by_id("clip")
}

fn music() -> ClipRef {
    ClipRef::by_id("music")
}

/// One row per group applies through a validated turn: Text,
/// Style, Layout, Motion, Grade, Effects, Overlays, Media, and
/// Audio FX each write a live value.
#[test]
fn rows_cover_every_group() {
    let dir = tmp("groups");
    project(&dir);
    let cases = [
        (title(), "font-size", "32", "32"),
        (card(), "background-color", "#00ff00", "#00ff00"),
        (card(), "x", "12", "12"),
        (card(), "filter-blur", "2", "2"),
        (card(), "grade-exposure", "1", "1"),
        (card(), "effect-amount", "0.5", "0.5"),
        (card(), "visible", "false", "false"),
        (clip(), "rate", "2", "2"),
        (clip(), "volume", "-6", "-6"),
    ];
    for (target, property, value, live) in cases {
        let receipt = row(&dir, target, property, value)
            .unwrap_or_else(|e| panic!("row {property} refused: {e}"));
        assert!(
            receipt.contains(live),
            "row {property}: receipt {receipt:?} misses live {live:?}"
        );
    }
    let session = open(&dir);
    let comp = session.effective_composition();
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "font-size").as_deref(),
        Some("32")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[1], "background-color").as_deref(),
        Some("#00ff00")
    );
}

/// Row edits undo through the shared history: the exact prior value
/// returns, not an approximation.
#[test]
fn row_edit_undoes_to_prior_value() {
    let dir = tmp("undo");
    project(&dir);
    let mut session = open(&dir);
    let before = sdk::row_live_value(&session.effective_composition().clips[0], "font-size")
        .expect("live value");
    assert_eq!(before, "24");
    session
        .apply(EditOp::SetRow {
            target: title(),
            property: "font-size".to_string(),
            value: "64".to_string(),
        })
        .expect("row applies");
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "font-size").as_deref(),
        Some("64")
    );
    session.undo().expect("undo restores");
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "font-size").as_deref(),
        Some("24")
    );
}

/// Row keys stay CSS-like hyphenated: underscores read as hyphens
/// and upper case folds.
#[test]
fn row_keys_are_css_like() {
    let dir = tmp("canon");
    project(&dir);
    row(&dir, title(), "FONT_SIZE", "40").expect("upper case folds");
    row(&dir, card(), "border_radius", "8").expect("underscores read as hyphens");
    let session = open(&dir);
    let comp = session.effective_composition();
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "font-size").as_deref(),
        Some("40")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[1], "border-radius").as_deref(),
        Some("8")
    );
}

/// `font-family` keeps comma try-order untouched: unknown families
/// pass through fail-closed, empty entries refuse.
#[test]
fn font_family_try_order_is_fail_closed() {
    let dir = tmp("family");
    project(&dir);
    row(&dir, title(), "font-family", "Alpha, Beta Gamma").expect("try-order stores");
    let session = open(&dir);
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "font-family").as_deref(),
        Some("Alpha, Beta Gamma")
    );
    let before = tree_bytes(&dir);
    let err = row(&dir, title(), "font-family", "Alpha,, Beta").expect_err("empty entry refuses");
    assert!(err.contains("E_SDK_BAD_VALUE"), "rule: {err}");
    assert!(err.contains("font-family"), "location: {err}");
    assert_eq!(tree_bytes(&dir), before, "prior value kept");
}

/// Clearing an optional row inherits the KDL omission default.
#[test]
fn clearing_optional_rows_inherits() {
    let dir = tmp("clear");
    project(&dir);
    row(&dir, title(), "font-family", "Alpha").expect("set");
    row(&dir, title(), "font-family", "").expect("clear inherits");
    row(&dir, title(), "line-height", "30").expect("set");
    row(&dir, title(), "line-height", "").expect("clear inherits");
    let session = open(&dir);
    let comp = session.effective_composition();
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "font-family").as_deref(),
        Some("")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "line-height").as_deref(),
        Some("")
    );
}

/// Render-deferred rows refuse naming their milestone.
#[test]
fn deferred_rows_refuse_with_milestone() {
    let dir = tmp("deferred");
    project(&dir);
    for property in [
        "filter",
        "transform-3d",
        "rotate-x",
        "perspective",
        "origin",
    ] {
        let before = tree_bytes(&dir);
        let err = row(&dir, card(), property, "1").expect_err("deferred refuses");
        assert!(err.contains("E_SDK_UNSUPPORTED"), "rule: {err}");
        assert!(err.contains("milestone"), "milestone named: {err}");
        assert_eq!(tree_bytes(&dir), before, "nothing staged");
    }
}

/// Wrong-kind rows refuse with rule and location keeping the prior
/// value: `font-size` on a box, `background-color` on text,
/// `object-fit` on text, `volume` on text, `z-index` on audio.
#[test]
fn wrong_kind_rows_refuse_located() {
    let dir = tmp("kind");
    project(&dir);
    let cases = [
        (card(), "font-size", "32"),
        (title(), "background-color", "#00ff00"),
        (title(), "object-fit", "cover"),
        (title(), "volume", "-6"),
        (music(), "z-index", "3"),
        (music(), "grade-exposure", "1"),
        (title(), "width", "50"),
        (clip(), "trim-start-x", "1"),
    ];
    for (target, property, value) in cases {
        let before = tree_bytes(&dir);
        let err = row(&dir, target, property, value).expect_err("wrong kind refuses");
        assert!(
            err.contains("E_SDK_UNSUPPORTED") || err.contains("E_SDK_BAD_VALUE"),
            "rule: {err}"
        );
        assert!(err.contains(property), "location names row: {err}");
        assert_eq!(tree_bytes(&dir), before, "prior kept for {property}");
    }
}

/// Bad values refuse with rule and location per family: color,
/// enum, range, fade past the window, empty value, rate outside
/// 0.1 to 10, volume past +12 dB.
#[test]
fn bad_values_refuse_per_family() {
    let dir = tmp("values");
    project(&dir);
    let cases = [
        (title(), "font-size", "-4"),
        (title(), "font-size", "huge"),
        (title(), "color", "red"),
        (title(), "text-align", "justify"),
        (title(), "font-style", "black"),
        (card(), "border-width", "-1"),
        (card(), "opacity", "2"),
        (card(), "mix-blend-mode", "vivid"),
        (card(), "effect-amount", "1.5"),
        (title(), "value", ""),
        (clip(), "volume", "13"),
        (clip(), "fade-in", "99"),
        (clip(), "delay", "5"),
        (clip(), "delay-feedback", "2"),
        (clip(), "reverb", "-1"),
        (clip(), "rate", "0.05"),
        (clip(), "rate", "11"),
    ];
    for (target, property, value) in cases {
        let before = tree_bytes(&dir);
        let err = row(&dir, target, property, value).expect_err("bad value refuses");
        assert!(
            err.contains("E_SDK_BAD_VALUE"),
            "rule for {property}: {err}"
        );
        assert!(err.contains(property), "location for {property}: {err}");
        assert_eq!(
            tree_bytes(&dir),
            before,
            "prior kept for {property}={value}"
        );
    }
}

/// An unknown row key refuses listing the accepted rows.
#[test]
fn unknown_row_key_lists_accepted() {
    let dir = tmp("unknown");
    project(&dir);
    let err = row(&dir, title(), "font-weight", "700").expect_err("unknown refuses");
    assert!(err.contains("E_SDK_BAD_VALUE"), "rule: {err}");
    assert!(err.contains("font-size"), "accepted rows listed: {err}");
}

/// Effect rows need an effect: a clip without one refuses with the
/// rule instead of inventing a look.
#[test]
fn effect_amount_without_effect_refuses() {
    let dir = tmp("noeffect");
    project(&dir);
    let err = row(&dir, title(), "effect-amount", "0.5").expect_err("no effect refuses");
    assert!(err.contains("E_SDK_UNSUPPORTED"), "rule: {err}");
    assert!(err.contains("effect"), "location: {err}");
}

/// A row turn persists in the delta and replays on reopen keeping
/// its position, and serializes as plain data.
#[test]
fn row_turn_persists_and_replays() {
    let dir = tmp("persist");
    project(&dir);
    let op = EditOp::SetRow {
        target: title(),
        property: "color".to_string(),
        value: "#123456".to_string(),
    };
    let raw = serde_json::to_value(&op).expect("serializes");
    assert_eq!(raw.get("op").and_then(|o| o.as_str()), Some("setrow"));
    let back: EditOp = serde_json::from_value(raw).expect("round-trips");
    assert_eq!(back.summary(), op.summary());
    let mut session = open(&dir);
    session.apply(op).expect("applies");
    session.persist().expect("persists");
    drop(session);
    let session = open(&dir);
    assert_eq!(
        sdk::row_live_value(&session.effective_composition().clips[0], "color").as_deref(),
        Some("#123456")
    );
}

/// Select rows (`text-align`, `font-style`, `object-fit`,
/// `mix-blend-mode`) accept exactly their vocabularies.
#[test]
fn select_rows_accept_vocabularies() {
    let dir = tmp("selects");
    project(&dir);
    row(&dir, title(), "text-align", "center").expect("center");
    row(&dir, title(), "font-style", "bold-italic").expect("bold-italic");
    row(&dir, clip(), "object-fit", "cover").expect("cover");
    row(&dir, card(), "mix-blend-mode", "multiply").expect("multiply");
    let session = open(&dir);
    let comp = session.effective_composition();
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "text-align").as_deref(),
        Some("center")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[0], "font-style").as_deref(),
        Some("bold-italic")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[2], "object-fit").as_deref(),
        Some("cover")
    );
    assert_eq!(
        sdk::row_live_value(&comp.clips[1], "mix-blend-mode").as_deref(),
        Some("multiply")
    );
}
