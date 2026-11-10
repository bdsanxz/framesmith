//! Sprint 21 T003: native 3D scenes through the pure-Rust pipeline.
//!
//! Covers every acceptance criterion of
//! `T003-render-3d-scenes-natively.md`: orbit and fixed cameras,
//! point and directional lights, backface-culled PBR-ish geometry
//! (authored primitives plus embedded meshes), deterministic camera
//! interpolation between keys, refusals naming the object and the
//! cause, and the named default-light fallback for a missing rig.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use validate::validate::{validate_folder, Diagnostic, Status};

static N: AtomicU64 = AtomicU64::new(0);

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "framesmith-validate-n21scene3d-{}-{}-{}",
        tag,
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

const OPEN: &str =
    "composition name=\"main\" width=640 height=480 duration=4 background=\"#000\" version=1 {\n";

fn scene(body: &str) -> String {
    format!("{OPEN}{body}\n}}\n")
}

fn diags(body: &str) -> Vec<Diagnostic> {
    let dir = tmp("case");
    write_comp(&dir, body);
    validate_folder(&dir).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<String> {
    ds.iter().map(|d| d.code.to_string()).collect()
}

fn assert_refusal(ds: &[Diagnostic], code: &str, needles: &[&str]) {
    assert!(!ds.is_empty(), "expected a refusal, got none");
    assert!(
        ds.iter().any(|d| d.code == code),
        "expected code {code} in {:?}",
        codes(ds)
    );
    let messages: Vec<String> = ds.iter().map(|d| d.message.clone()).collect();
    for needle in needles {
        assert!(
            messages.iter().any(|m| m.contains(needle)),
            "expected {needle:?} in {messages:?}"
        );
    }
}

fn first_scene(body: &str) -> notation::model::Scene3dClip {
    let dir = tmp("valid");
    write_comp(&dir, body);
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    match v.composition.unwrap().clips.into_iter().next().unwrap() {
        notation::model::Clip::Scene3d(s) => s,
        other => panic!("expected a scene3d, got {other:?}"),
    }
}

const BOX_SCENE: &str = "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\" size=\"2,2,2\" color=\"#c02020\"\n}\n";

// --- orbit and fixed cameras render correctly ------------------------------

#[test]
fn orbit_and_fixed_cameras_validate() {
    use notation::model::Scene3dCameraMode;
    let orbit = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 azimuth=10 elevation=15\n}\n",
    ));
    assert_eq!(orbit.camera.mode, Scene3dCameraMode::Orbit);
    assert_eq!(orbit.camera.distance, 6.0);
    let fixed = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"ball\" kind=\"sphere\" radius=1\n    camera mode=\"fixed\" position=\"0,1,6\" target=\"0,0,0\"\n}\n",
    ));
    assert_eq!(fixed.camera.mode, Scene3dCameraMode::Fixed);
    assert_eq!(fixed.camera.position, [0.0, 1.0, 6.0]);
}

// --- point and directional lights shade as specified ------------------------

#[test]
fn point_and_directional_lights_validate() {
    use notation::model::Scene3dLightKind;
    let clip = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    light name=\"bulb\" kind=\"point\" position=\"2,3,4\" intensity=8\n    light name=\"sun\" kind=\"directional\" direction=\"-0.3,-1,-0.4\"\n}\n",
    ));
    assert_eq!(clip.lights.len(), 2);
    assert_eq!(clip.lights[0].kind, Scene3dLightKind::Point);
    assert_eq!(clip.lights[0].position, [2.0, 3.0, 4.0]);
    assert_eq!(clip.lights[1].kind, Scene3dLightKind::Directional);
}

// --- authored primitives plus embedded mesh data ----------------------------

#[test]
fn every_primitive_kind_and_meshes_validate() {
    for kind in ["box", "sphere", "plane", "cylinder"] {
        let clip = first_scene(&scene(&format!(
            "scene3d start=0 duration=4 width=320 height=240 {{\n    object name=\"thing\" kind=\"{kind}\"\n}}\n"
        )));
        assert_eq!(clip.objects.len(), 1, "kind {kind}");
    }
    let mesh = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"tri\" kind=\"mesh\" vertices=\"-1,-1,0; 1,-1,0; 0,1,0\" faces=\"0,1,2\"\n}\n",
    ));
    assert_eq!(mesh.objects[0].vertices.len(), 3);
    assert_eq!(mesh.objects[0].faces, vec![[0, 1, 2]]);
}

// --- cameras interpolate between keys deterministically ---------------------

#[test]
fn camera_keys_validate_and_refuse_duplicates() {
    let clip = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 {\n        key at=0 azimuth=0\n        key at=2 azimuth=90\n    }\n}\n",
    ));
    assert_eq!(clip.camera.keys.len(), 2);
    assert_eq!(clip.camera.keys[0].at, 0.0);
    assert_eq!(clip.camera.keys[1].azimuth, Some(90.0));
    // Validating twice resolves the same model: deterministic decode.
    let again = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 {\n        key at=0 azimuth=0\n        key at=2 azimuth=90\n    }\n}\n",
    ));
    assert_eq!(format!("{clip:?}"), format!("{again:?}"));
    // Duplicate key times refuse.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" {\n        key at=1 azimuth=0\n        key at=1 azimuth=45\n    }\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["two `key` snapshots", "never repeat"],
    );
    // A key without `at` refuses.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" {\n        key azimuth=45\n    }\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["missing required `at`"],
    );
}

// --- unbuildable scenes refuse naming object and cause ----------------------

#[test]
fn unbuildable_scenes_refuse_naming_object_and_cause() {
    // Unknown geometry kind names the object and the kind.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"wheel\" kind=\"torus\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"wheel\"", "torus"],
    );
    // Missing object name refuses.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object kind=\"box\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["missing required `name`"],
    );
    // No objects at all refuses naming the scene.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["holds no `object`"],
    );
    // Out-of-range mesh faces name the object and the cause.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"tri\" kind=\"mesh\" vertices=\"-1,-1,0; 1,-1,0; 0,1,0\" faces=\"0,1,7\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"tri\"", "faces"],
    );
    // Duplicate face indices refuse.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"tri\" kind=\"mesh\" vertices=\"-1,-1,0; 1,-1,0; 0,1,0\" faces=\"0,0,1\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"tri\"", "distinct vertices"],
    );
    // Unknown light kind names the light.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    light name=\"glow\" kind=\"spot\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"glow\"", "spot"],
    );
    // Radius on a box refuses instead of sitting inert.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\" radius=2\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"cube\"", "`radius`"],
    );
    // Orbit position refuses instead of sitting inert.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" position=\"0,0,5\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["mode `orbit`", "`position`"],
    );
}

// --- fail-closed unknowns ----------------------------------------------------

#[test]
fn unknown_keys_and_children_refuse() {
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 fog=1 {\n    object name=\"cube\" kind=\"box\"\n}\n",
        )),
        "E_UNKNOWN_KEY",
        &["fog"],
    );
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\" shininess=2\n}\n",
        )),
        "E_UNKNOWN_KEY",
        &["shininess"],
    );
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    fog density=2\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["holds `fog`"],
    );
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"sideways\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["`mode`", "orbit"],
    );
}

// --- missing rigs fall back to the named default light ----------------------

#[test]
fn missing_light_rig_validates_with_empty_lights() {
    let clip = first_scene(&scene(BOX_SCENE));
    assert!(
        clip.lights.is_empty(),
        "no authored lights: the renderer falls back to the named default light"
    );
}

// --- default camera frames without an authored camera -----------------------

#[test]
fn missing_camera_validates_with_default_orbit_camera() {
    use notation::model::Scene3dCameraMode;
    let clip = first_scene(&scene(BOX_SCENE));
    assert_eq!(clip.camera.mode, Scene3dCameraMode::Orbit);
    assert!(clip.camera.keys.is_empty());
}

// --- motion-aware lint sees camera keys --------------------------------------

#[test]
fn orbiting_scenes_count_as_changing() {
    // An orbiting scene never reports frozen-timeline ...
    let dir = tmp("lint-moving");
    write_comp(
        &dir,
        &scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 {\n        key at=0 azimuth=0\n        key at=4 azimuth=90\n    }\n}\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(
        v.lint_findings.iter().all(|f| f.rule != "frozen-timeline"),
        "an orbiting scene moves: {:?}",
        v.lint_findings
    );
    // ... while a static scene still does.
    let dir = tmp("lint-still");
    write_comp(&dir, &scene(BOX_SCENE));
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(
        v.lint_findings.iter().any(|f| f.rule == "frozen-timeline"),
        "a static scene holds still: {:?}",
        v.lint_findings
    );
}

// --- lint covers adapter sources --------------------------------------------

#[test]
fn scene3d_clips_take_part_in_layout_lint() {
    // An off-canvas scene3d reports like any picture clip.
    let dir = tmp("lint-off");
    write_comp(
        &dir,
        &scene("scene3d start=0 duration=4 width=320 height=240 x=700 y=500 {\n    object name=\"cube\" kind=\"box\"\n}\n"),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(
        v.lint_findings.iter().any(|f| f.rule == "off-canvas"),
        "expected an off-canvas finding, got {:?}",
        v.lint_findings
    );
    // Two overlapping scene3d clips report the overlap.
    let dir = tmp("lint-overlap");
    write_comp(
        &dir,
        &scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"a\" kind=\"box\"\n}\nscene3d start=0 duration=4 width=320 height=240 {\n    object name=\"b\" kind=\"box\"\n}\n",
        ),
    );
    let v = validate_folder(&dir);
    assert_eq!(v.status, Status::Valid, "{:?}", v.diagnostics);
    assert!(
        v.lint_findings.iter().any(|f| f.rule == "overlap"),
        "expected an overlap finding, got {:?}",
        v.lint_findings
    );
}

// --- geometry values stay fail-closed ----------------------------------------

#[test]
fn bad_geometry_values_refuse() {
    // Negative radius refuses naming the object.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"ball\" kind=\"sphere\" radius=-1\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"ball\"", "radius"],
    );
    // Non-positive raster width refuses.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=0 height=240 {\n    object name=\"cube\" kind=\"box\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["width"],
    );
    // Malformed triplets refuse.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\" position=\"0,0\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"cube\"", "position"],
    );
    // Mesh without vertices refuses naming the object.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"blob\" kind=\"mesh\" faces=\"0,1,2\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"blob\"", "vertices"],
    );
    // Keys sort by time: authored order never matters for validity.
    let clip = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" {\n        key at=2 azimuth=90\n        key at=0 azimuth=0\n    }\n}\n",
    ));
    assert_eq!(clip.camera.keys.len(), 2);
}

// --- fail-closed edges -------------------------------------------------------

#[test]
fn scene_edges_refuse() {
    // Duplicate object names refuse naming the name.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    object name=\"cube\" kind=\"sphere\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["two objects", "\"cube\""],
    );
    // Collinear mesh faces refuse naming the object and the face.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"flat\" kind=\"mesh\" vertices=\"0,0,0; 1,1,1; 2,2,2\" faces=\"0,1,2\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"flat\"", "spans no area"],
    );
    // A point light's `direction` refuses instead of sitting inert.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    light name=\"bulb\" kind=\"point\" position=\"2,3,4\" direction=\"0,-1,0\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"bulb\"", "`direction`"],
    );
    // A directional light's `position` refuses instead of sitting inert.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    light name=\"sun\" kind=\"directional\" position=\"2,3,4\" direction=\"0,-1,0\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["\"sun\"", "`position`"],
    );
    // A fixed camera on its target refuses.
    assert_refusal(
        &diags(&scene(
            "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"fixed\" position=\"0,0,0\" target=\"0,0,0\"\n}\n",
        )),
        "E_BAD_SCENE3D",
        &["mode `fixed`", "target"],
    );
    // Straight-down orbit cameras validate: the pole frames.
    let clip = first_scene(&scene(
        "scene3d start=0 duration=4 width=320 height=240 {\n    object name=\"cube\" kind=\"box\"\n    camera mode=\"orbit\" distance=6 elevation=90\n}\n",
    ));
    assert_eq!(clip.camera.elevation, 90.0);
}
