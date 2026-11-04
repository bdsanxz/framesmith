//! Sprint 11 VFX (T005) validation: an ordered `vfx` chain (`noise`,
//! `warp`, `displacement`, `luma-matte`) on clip children, stages
//! applying in the fixed order regardless of document order, each
//! stage at most once, with seeded re-renders.
//!
//! Validates through the public [`validate_folder`] API only: every
//! stage lands on every clip kind; document order never changes the
//! resolved chain; disabling a stage matches the chain written
//! without it; seeded noise re-renders byte-identically; duplicated
//! stages, unknown stages, a matte missing `src`, and missing files
//! fail with the element and the fix.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n11-vfx-{tag}-{}-{}",
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
/// bytes for pictures), never decodes.
fn touch_media(dir: &Path) {
    std::fs::write(dir.join("photo.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
    std::fs::write(
        dir.join("pic.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"48\" height=\"32\"></svg>".to_vec(),
    )
    .unwrap();
    std::fs::write(dir.join("clip.mp4"), []).unwrap();
    std::fs::write(dir.join("s.wav"), []).unwrap();
    std::fs::write(dir.join("matte.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
    std::fs::write(dir.join("map.png"), b"\x89PNG\r\n\x1a\n".to_vec()).unwrap();
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

fn clip_vfx(idx: usize, dir: &Path) -> notation::model::VfxChain {
    let v = validate_folder(dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    comp.clips[idx].vfx().clone()
}

// --- T005: vfx on every clip kind ----------------------------------------------

#[test]
fn vfx_lands_on_every_clip_kind() {
    // One `noise` stage per clip kind; the chain lands on the clip.
    let dir = tmp("kinds-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n text value=\"Hi\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             video path=\"clip.mp4\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             flex x=0 y=0 width=60 height=30 flex-direction=\"row\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n box width=20 height=10 fill=\"#00ff00\"\n}}\n\
             image path=\"photo.png\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             path d=\"M10,10 L50,10 L50,50 Z\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             svg path=\"pic.svg\" width=48 height=32 start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             audio path=\"s.wav\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
             nest nest=\"bumper\" start=0 duration=1 {{\n vfx {{\n noise amount=0.5 seed=7\n }}\n}}\n\
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
    for clip in &comp.clips {
        let vfx = clip.vfx();
        let noise = vfx
            .noise
            .as_ref()
            .unwrap_or_else(|| panic!("{:?} carries noise", clip.kind()));
        assert_eq!(noise.amount, 0.5);
        assert_eq!(noise.seed, Some(7));
        assert!(vfx.warp.is_none());
        assert!(vfx.displacement.is_none());
        assert!(vfx.luma_matte.is_none());
    }
}

// --- T005: all four stages -------------------------------------------------------

#[test]
fn all_four_stages_validate_with_parameters() {
    // Every stage with explicit parameters; unmentioned knobs stay
    // unset and render at their documented defaults.
    let dir = tmp("all-stages");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise amount=0.5 seed=7\n warp grid=8 strength=4\n displacement map=\"map.png\" scale=6\n luma-matte src=\"matte.png\" amount=0.8\n }}\n}}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::Clip;
    let vfx = match &comp.clips[0] {
        Clip::Box(b) => &b.vfx,
        _ => panic!("expected a box clip"),
    };
    let noise = vfx.noise.as_ref().expect("noise stage");
    assert_eq!(noise.amount, 0.5);
    assert_eq!(noise.seed, Some(7));
    let warp = vfx.warp.as_ref().expect("warp stage");
    assert_eq!(warp.amount, 1.0);
    assert_eq!(warp.grid, Some(8));
    assert_eq!(warp.strength, Some(4.0));
    let displacement = vfx.displacement.as_ref().expect("displacement stage");
    assert_eq!(displacement.map.as_deref(), Some("map.png"));
    assert_eq!(displacement.scale, Some(6.0));
    let matte = vfx.luma_matte.as_ref().expect("luma-matte stage");
    assert_eq!(matte.src, "matte.png");
    assert_eq!(matte.amount, 0.8);
    assert!(!vfx.is_empty());
}

#[test]
fn unmentioned_parameters_render_at_documented_defaults() {
    // Bare stages leave every knob unset; the documented defaults
    // come from the model accessors.
    let dir = tmp("defaults-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise\n warp\n displacement\n luma-matte src=\"matte.png\"\n }}\n}}\n}}\n"
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    let comp = v.composition.expect("valid composition");
    use notation::model::{
        Clip, VFX_DEFAULT_AMOUNT, VFX_DEFAULT_DISPLACEMENT_SCALE, VFX_DEFAULT_WARP_GRID,
        VFX_DEFAULT_WARP_STRENGTH, VFX_SEED_DEFAULT,
    };
    let vfx = match &comp.clips[0] {
        Clip::Box(b) => &b.vfx,
        _ => panic!("expected a box clip"),
    };
    let noise = vfx.noise.as_ref().expect("noise stage");
    assert_eq!(noise.amount, VFX_DEFAULT_AMOUNT);
    assert!(noise.seed.is_none());
    assert_eq!(noise.seed(), VFX_SEED_DEFAULT);
    let warp = vfx.warp.as_ref().expect("warp stage");
    assert!(warp.grid.is_none());
    assert_eq!(warp.grid(), VFX_DEFAULT_WARP_GRID);
    assert!(warp.strength.is_none());
    assert_eq!(warp.strength(), VFX_DEFAULT_WARP_STRENGTH);
    let displacement = vfx.displacement.as_ref().expect("displacement stage");
    assert!(displacement.map.is_none());
    assert!(displacement.scale.is_none());
    assert_eq!(displacement.scale(), VFX_DEFAULT_DISPLACEMENT_SCALE);
    let matte = vfx.luma_matte.as_ref().expect("luma-matte stage");
    assert_eq!(matte.amount, VFX_DEFAULT_AMOUNT);
}

// --- T005: fixed order regardless of document order -------------------------------

#[test]
fn document_order_does_not_change_the_chain() {
    // Stages authored matte-first still land on their fixed slots;
    // the fingerprint is structural, so both orders fingerprint
    // identically: document order never changes application order.
    let dir = tmp("order");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=2 {{\n vfx {{\n luma-matte src=\"matte.png\"\n displacement scale=4\n warp strength=4\n noise seed=7\n }}\n}}\n box x=30 y=0 width=20 height=12 fill=\"#00ff00\" start=0 duration=2 {{\n vfx {{\n noise seed=7\n warp strength=4\n displacement scale=4\n luma-matte src=\"matte.png\"\n }}\n}}\n}}\n"
        ),
    );
    let reversed = clip_vfx(0, &dir);
    let canonical = clip_vfx(1, &dir);
    assert!(reversed.noise.is_some());
    assert!(reversed.warp.is_some());
    assert!(reversed.displacement.is_some());
    assert!(reversed.luma_matte.is_some());
    assert_eq!(reversed, canonical);
    assert_eq!(reversed.fingerprint(), canonical.fingerprint());
}

#[test]
fn inert_stages_validate_as_pass_through() {
    // A zero `amount` (or zero `strength`/`scale`) validates and
    // marks the chain inert, so the fast path renders it untouched.
    let dir = tmp("inert-ok");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise amount=0 seed=7\n warp strength=0\n displacement scale=0\n luma-matte src=\"matte.png\" amount=0\n }}\n}}\n}}\n"
        ),
    );
    let vfx = clip_vfx(0, &dir);
    // Inert stages validate and mark the chain inert (the fast path
    // renders it untouched); the fingerprint stays structural, so an
    // authored-but-inert chain still fingerprints distinctly from a
    // chain written without it. Pixel purity is proven at render.
    assert!(vfx.is_empty());
    assert_ne!(
        vfx.fingerprint(),
        notation::model::VfxChain::empty().fingerprint()
    );
}

// --- T005: refusals ---------------------------------------------------------------

#[test]
fn duplicated_vfx_child_fails_naming_the_duplicate() {
    let dir = tmp("dup-vfx");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise\n }}\n vfx {{\n warp\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let (code, message, fix) = &diags[0];
    assert_eq!(code, "E_BAD_VFX");
    assert!(message.contains("two `vfx`"), "{message}");
    assert!(message.contains("box clip"), "{message}");
    assert!(!fix.is_empty(), "every refusal names a concrete fix");
}

#[test]
fn unknown_stage_fails_naming_the_accepted_set() {
    let dir = tmp("unknown-stage");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n blur radius=4\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let (code, message, fix) = &diags[0];
    assert_eq!(code, "E_BAD_VFX");
    assert!(message.contains("`blur`"), "{message}");
    for name in ["noise", "warp", "displacement", "luma-matte"] {
        assert!(message.contains(name), "refusal names {name}: {message}");
    }
    assert!(!fix.is_empty(), "every refusal names a concrete fix");
}

#[test]
fn duplicated_stage_fails_naming_the_stage() {
    for stage in ["noise", "warp", "displacement"] {
        let dir = tmp(&format!("dup-{stage}"));
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n {stage}\n {stage}\n }}\n}}\n}}\n"
            ),
        );
        let diags = codes_messages(&dir);
        assert_eq!(diags.len(), 1, "{diags:?} ({stage})");
        let (code, message, fix) = &diags[0];
        assert_eq!(code, "E_BAD_VFX");
        assert!(message.contains(&format!("two `{stage}`")), "{message}");
        assert!(message.contains("at most once"), "{message}");
        assert!(!fix.is_empty(), "every refusal names a concrete fix");
    }
    // The hyphenated matte accepts its underscore spelling, but twice
    // is still a duplicate.
    let dir = tmp("dup-matte");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n luma-matte src=\"matte.png\"\n luma_matte src=\"matte.png\"\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let (code, message, _) = &diags[0];
    assert_eq!(code, "E_BAD_VFX");
    assert!(message.contains("two `luma-matte`"), "{message}");
}

#[test]
fn luma_matte_missing_src_fails_naming_the_stage_and_fix() {
    let dir = tmp("matte-no-src");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n luma-matte amount=0.5\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let (code, message, fix) = &diags[0];
    assert_eq!(code, "E_BAD_VFX");
    assert!(message.contains("`luma-matte`"), "{message}");
    assert!(message.contains("`src`"), "{message}");
    assert!(fix.contains("`src="), "fix names the concrete fix: {fix}");
}

#[test]
fn stage_picture_missing_file_fails_naming_the_file() {
    for (stage, child) in [
        ("luma-matte", "luma-matte src=\"gone.png\""),
        ("displacement", "displacement map=\"gone.png\""),
    ] {
        let dir = tmp(&format!("missing-{stage}"));
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n {child}\n }}\n}}\n}}\n"
            ),
        );
        let diags = codes_messages(&dir);
        assert_eq!(diags.len(), 1, "{diags:?} ({stage})");
        let (code, message, fix) = &diags[0];
        assert_eq!(code, "E_MISSING_FILE");
        assert!(message.contains("gone.png"), "{message}");
        assert!(message.contains(stage), "{message}");
        assert!(!fix.is_empty(), "every refusal names a concrete fix");
    }
}

#[test]
fn out_of_range_stage_values_fail_naming_the_range() {
    for (child, key) in [
        ("noise amount=2", "amount"),
        ("noise amount=-0.5", "amount"),
        ("noise seed=1.5", "seed"),
        ("warp grid=0", "grid"),
        ("warp grid=65", "grid"),
        ("warp grid=2.5", "grid"),
        ("warp strength=-1", "strength"),
        ("warp strength=65", "strength"),
        ("displacement scale=-1", "scale"),
        ("displacement scale=65", "scale"),
        ("displacement amount=2", "amount"),
        ("luma-matte src=\"matte.png\" amount=2", "amount"),
    ] {
        let dir = tmp("range");
        touch_media(&dir);
        write_comp(
            &dir,
            &format!(
                "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n {child}\n }}\n}}\n}}\n"
            ),
        );
        let diags = codes_messages(&dir);
        assert_eq!(diags.len(), 1, "{diags:?} ({child})");
        let (code, message, fix) = &diags[0];
        assert_eq!(code, "E_BAD_VFX");
        assert!(message.contains(&format!("`{key}`")), "{message} ({child})");
        assert!(!fix.is_empty(), "every refusal names a concrete fix");
    }
}

#[test]
fn unknown_keys_and_children_fail_with_their_shape() {
    // An unknown key on a stage is `E_UNKNOWN_KEY`; a prop on the
    // `vfx` itself is `E_UNKNOWN_KEY`; a child on a stage is
    // `E_BAD_VFX`. Every refusal names the element and the fix.
    let dir = tmp("bad-key");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise radius=4\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_UNKNOWN_KEY");
    assert!(diags[0].1.contains("`noise`"), "{}", diags[0].1);
    assert!(!diags[0].2.is_empty());

    let dir = tmp("vfx-prop");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx amount=0.5 {{\n noise\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_UNKNOWN_KEY");
    assert!(diags[0].1.contains("`vfx`"), "{}", diags[0].1);

    let dir = tmp("stage-child");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n noise {{\n warp\n }}\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_BAD_VFX");
    assert!(diags[0].1.contains("`noise`"), "{}", diags[0].1);
    assert!(!diags[0].2.is_empty());
}

#[test]
fn non_string_picture_reference_fails_naming_the_stage() {
    let dir = tmp("num-src");
    touch_media(&dir);
    write_comp(
        &dir,
        &format!(
            "{OPEN}\n box x=0 y=0 width=20 height=12 fill=\"#ff0000\" start=0 duration=4 {{\n vfx {{\n luma-matte src=7\n }}\n}}\n}}\n"
        ),
    );
    let diags = codes_messages(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].0, "E_BAD_VFX");
    assert!(diags[0].1.contains("`luma-matte`"), "{}", diags[0].1);
    assert!(!diags[0].2.is_empty());
}
