//! Sprint 11 effects (T004) validation: eighteen named `effect`
//! presets with `amount` and preset-specific parameters on clip
//! children, applied after the grade chain in document order.
//!
//! Validates through the public [`validate_folder`] API only: every
//! preset lands on every clip kind; half and zero amounts validate;
//! unmentioned parameters stay unset (rendering at their documented
//! defaults); document order is the application order; unknown
//! presets, out-of-range amounts, and malformed parameters fail with
//! the element and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-effect-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_comp(dir: &Path, body: &str) {
    std::fs::write(dir.join("composition.kdl"), body).unwrap();
}

/// Dummy media files: validation checks presence (and image magic
/// bytes), never decodes.
fn touch_media(dir: &Path) {
    std::fs::write(dir.join("photo.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
    std::fs::write(
        dir.join("pic.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"48\" height=\"32\"></svg>".to_vec(),
    )
    .unwrap();
    std::fs::write(dir.join("clip.mp4"), []).unwrap();
    std::fs::write(dir.join("s.wav"), []).unwrap();
}

const OPEN: &str =
    "composition name=\"main\" root=true width=160 height=120 duration=4 background=\"#000\" version=1 {\n";

fn codes_messages(dir: &Path) -> Vec<(String, String, String)> {
    validate_folder(dir)
        .diagnostics
        .iter()
        .map(|d| (d.code.to_string(), d.message.clone(), d.fix.clone()))
        .collect()
}

// --- T004: presets on every clip kind ------------------------------------------

#[test]
fn effects_land_on_every_clip_kind() {
    // One `grayscale` effect per clip kind; the effect lands on the clip.
    let dir = tmp("kinds-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n             text value=\"Hi\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             video path=\"clip.mp4\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             flex x=0 y=0 width=60 height=30 flex-direction=\"row\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n box width=20 height=10 fill=\"#00ff00\"\n}}\n\
             image path=\"photo.png\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             path d=\"M10,10 L50,10 L50,50 Z\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             svg path=\"pic.svg\" width=48 height=32 start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             audio path=\"s.wav\" start=0 duration=4 {{\n effect preset=\"grayscale\"\n}}\n\
             nest nest=\"bumper\" start=0 duration=1 {{\n effect preset=\"grayscale\"\n}}\n\
             }}\n\
             composition name=\"bumper\" width=64 height=64 duration=1 background=\"#000\" version=1 {{\n\
             text value=\"B\" start=0 duration=1\n\
             }}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 9);
    use notation::model::EffectPreset;
    for clip in &comp.clips {
        let effects = clip.effects();
        assert_eq!(effects.len(), 1, "{:?} carries one effect", clip.kind());
        assert_eq!(effects[0].preset, EffectPreset::Grayscale);
        assert_eq!(effects[0].amount, 1.0);
    }
}

// --- T004: all eighteen presets -----------------------------------------------

#[test]
fn all_eighteen_presets_validate() {
    // Every preset validates on a box with its documented parameters;
    // unmentioned parameters stay unset for their documented defaults.
    let dir = tmp("all-presets");
    touch_media(&dir);
    let mut body = OPEN.to_string();
    let cases = [
        "grayscale",
        "sepia",
        "invert",
        "brightness brightness=1.3",
        "contrast contrast=1.3",
        "hue-rotate angle=90",
        "saturate saturate=2",
        "blur radius=4",
        "sharpen strength=1",
        "vignette softness=0.5",
        "grain size=2 seed=7",
        "noise seed=7",
        "pixelate size=8",
        "posterize levels=4",
        "threshold threshold=0.5",
        "edge-detect strength=1",
        "emboss strength=1",
        "chromatic-aberration distance=2",
    ];
    for (i, effect) in cases.iter().enumerate() {
        body.push_str(&format!(
            "box x={} y=0 width=8 height=8 fill=\"#c86432\" start=0 duration=4 {{\n effect preset={}\n}}\n",
            i * 8,
            effect
                .split_once(' ')
                .map(|(p, rest)| format!("\"{p}\" {rest}"))
                .unwrap_or_else(|| format!("\"{effect}\"")),
        ));
    }
    body.push_str("}\n");
    write_comp(&dir, &body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    assert_eq!(comp.clips.len(), 18);
    use notation::model::{Clip, EffectPreset};
    let presets: Vec<EffectPreset> = comp
        .clips
        .iter()
        .map(|c| match c {
            Clip::Box(b) => b.effects[0].preset,
            _ => panic!("expected box clips"),
        })
        .collect();
    assert_eq!(
        presets,
        EffectPreset::ACCEPTED
            .iter()
            .map(|n| EffectPreset::parse(n).unwrap())
            .collect::<Vec<_>>()
    );
}

// --- T004: amounts -------------------------------------------------------------

#[test]
fn half_and_zero_amounts_validate() {
    let dir = tmp("amounts-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"blur\" radius=4 amount=0.5\n effect preset=\"invert\" amount=0\n}}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    let effects = match &comp.clips[0] {
        Clip::Box(b) => &b.effects,
        _ => panic!("expected a box clip"),
    };
    assert_eq!(effects.len(), 2);
    assert_eq!(effects[0].amount, 0.5);
    assert_eq!(effects[1].amount, 0.0);
    assert!(effects[1].is_empty());
}

// --- T004: defaults -------------------------------------------------------------

#[test]
fn unmentioned_parameters_render_at_documented_defaults() {
    // A bare preset leaves every parameter unset; the documented
    // defaults come from the model accessors.
    let dir = tmp("defaults-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"brightness\"\n effect preset=\"grain\"\n}}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::{
        Clip, EFFECT_DEFAULT_BRIGHTNESS, EFFECT_DEFAULT_GRAIN_SIZE, EFFECT_SEED_DEFAULT,
    };
    let effects = match &comp.clips[0] {
        Clip::Box(b) => &b.effects,
        _ => panic!("expected a box clip"),
    };
    assert_eq!(effects.len(), 2);
    assert!(effects[0].params.brightness.is_none());
    assert_eq!(effects[0].params.brightness(), EFFECT_DEFAULT_BRIGHTNESS);
    assert!(effects[1].params.size.is_none());
    assert!(effects[1].params.seed.is_none());
    assert_eq!(effects[1].params.grain_size(), EFFECT_DEFAULT_GRAIN_SIZE);
    assert_eq!(effects[1].params.seed(), EFFECT_SEED_DEFAULT);
}

// --- T004: document order -------------------------------------------------------

#[test]
fn document_order_decides_application_order() {
    // Stacked effects keep written order both ways round.
    for (first, second) in [("invert", "grayscale"), ("grayscale", "invert")] {
        let dir = tmp("order");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"{first}\"\n effect preset=\"{second}\"\n}}\n}}\n"
            ),
        );
        let v = validate_folder(&dir);
        assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
        let comp = v.composition.expect("valid composition");
        use notation::model::Clip;
        let names: Vec<&str> = match &comp.clips[0] {
            Clip::Box(b) => b.effects.iter().map(|e| e.preset.name()).collect(),
            _ => panic!("expected a box clip"),
        };
        assert_eq!(names, vec![first, second]);
    }
}

// --- T004: refusals ---------------------------------------------------------------

#[test]
fn unknown_preset_fails_naming_the_accepted_set() {
    let dir = tmp("unknown-preset");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"vintage\"\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let (code, message, fix) = &diags[0];
    assert_eq!(code, "E_BAD_EFFECT");
    assert!(message.contains("`effect`"), "{message}");
    assert!(message.contains("vintage"), "{message}");
    assert!(message.contains("grayscale"), "{message}");
    assert!(message.contains("chromatic-aberration"), "{message}");
    assert!(fix.contains("`effect`"), "{fix}");
    assert!(fix.contains("preset="), "{fix}");
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
}

#[test]
fn missing_preset_fails_naming_the_element() {
    let dir = tmp("missing-preset");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect amount=0.5\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_BAD_EFFECT");
    assert!(
        diags[0].1.contains("missing required `preset`"),
        "{}",
        diags[0].1
    );
    assert!(diags[0].2.contains("preset="), "{}", diags[0].2);
}

#[test]
fn amount_outside_zero_to_one_fails_naming_the_range() {
    for amount in ["2", "-0.5"] {
        let dir = tmp("bad-amount");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"blur\" amount={amount}\n}}\n}}\n"
            ),
        );
        let diags = codes_messages(&dir);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].0, "E_BAD_EFFECT");
        assert!(diags[0].1.contains("`blur`"), "{}", diags[0].1);
        assert!(diags[0].1.contains("0 to 1"), "{}", diags[0].1);
        assert!(diags[0].2.contains("amount="), "{}", diags[0].2);
    }
}

#[test]
fn malformed_parameters_fail_naming_preset_and_shape() {
    // Every bad parameter in one composition: each fails with
    // E_BAD_EFFECT naming its preset and the expected shape.
    let dir = tmp("bad-params");
    touch_media(&dir);
    let bad = [
        "effect preset=\"brightness\" brightness=\"hot\"",
        "effect preset=\"hue-rotate\" angle=400",
        "effect preset=\"blur\" radius=-1",
        "effect preset=\"sharpen\" strength=5",
        "effect preset=\"vignette\" softness=2",
        "effect preset=\"grain\" size=1.5",
        "effect preset=\"grain\" seed=\"x\"",
        "effect preset=\"noise\" seed=1.5",
        "effect preset=\"pixelate\" size=0",
        "effect preset=\"posterize\" levels=1",
        "effect preset=\"posterize\" levels=2.5",
        "effect preset=\"threshold\" threshold=1.5",
        "effect preset=\"emboss\" strength=-0.5",
        "effect preset=\"chromatic-aberration\" distance=40",
        "effect preset=\"grayscale\" radius=4",
        "effect preset=\"invert\" amount=\"half\"",
    ];
    let mut body = OPEN.to_string();
    for (i, effect) in bad.iter().enumerate() {
        body.push_str(&format!(
            "box x={} y=0 width=4 height=4 fill=\"#c86432\" start=0 duration=4 {{\n {effect}\n}}\n",
            i * 4,
        ));
    }
    body.push_str("}\n");
    write_comp(&dir, &body);
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), bad.len(), "{diags:?}");
    for (code, message, fix) in &diags {
        assert_eq!(code, "E_BAD_EFFECT", "{message}");
        assert!(message.contains("`effect`"), "{message}");
        assert!(fix.contains("`effect`"), "{fix}");
    }
    assert_eq!(validate_folder(&dir).status, Status::Invalid);
}

#[test]
fn unknown_effect_key_fails() {
    let dir = tmp("unknown-key");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"blur\" bluriness=4\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_UNKNOWN_KEY");
    assert!(diags[0].1.contains("bluriness"), "{}", diags[0].1);
}

#[test]
fn effect_children_fail() {
    let dir = tmp("effect-child");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n effect preset=\"invert\" {{\n grade preset=\"none\"\n}}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_BAD_EFFECT");
    assert!(diags[0].1.contains("`effect`"), "{}", diags[0].1);
}

#[test]
fn effect_inside_grade_fails_as_a_grade_child() {
    // `effect` is a clip child, not a grade stage.
    let dir = tmp("effect-in-grade");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n grade preset=\"none\" {{\n effect preset=\"invert\"\n}}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_BAD_GRADE");
}
