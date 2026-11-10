//! Native 3D scenes (Sprint 21 adapters, T003 / CAP-003).
//!
//! A `scene3d` clip maps its authored objects, camera, and lights to
//! the pure-Rust [`threed`] core and rasterizes one deterministic
//! frame at paint time: the camera resolves at the clip-local instant
//! (keys lerp, ends clamp), geometry builds from authored primitives
//! plus embedded meshes, and an empty light rig renders with the named
//! default light. Pure standard library: no browser, no JavaScript,
//! no network at render time. The finished raster enters the shared
//! image stack (order, opacity, motion, grade, effects, VFX) exactly
//! like a decoded picture, pre-composed at the evaluated raster size.
//!
//! The 3D projection fills the raster exactly (vertical `fov` sets the
//! scale, the aspect follows the raster), so every `fit` paints the
//! same pixels: `fit`/`placement` ride the clip for chrome uniformity
//! and are documented inert, never refused guesses.

use notation::model::{Scene3dCamera, Scene3dLight, Scene3dLightKind, Scene3dObject};

/// Synthetic path prefix of on-demand scene rasters (never a real
/// file, so no project image can collide with one in diagnostics).
pub const SCENE3D_PREFIX: &str = "scene3d-frames/scene";

/// Map one authored object to the [`threed`] core: primitives build
/// their tessellation from the owning kind's size fields, meshes ride
/// through with their embedded vertices and faces.
pub fn build_object(object: &Scene3dObject) -> threed::Object {
    let mesh = match object.kind {
        notation::model::Scene3dGeometry::Box => {
            threed::box_mesh(object.size[0], object.size[1], object.size[2])
        }
        notation::model::Scene3dGeometry::Sphere => {
            threed::sphere_mesh(object.radius, object.segments)
        }
        notation::model::Scene3dGeometry::Plane => {
            threed::plane_mesh(object.size[0], object.size[1])
        }
        notation::model::Scene3dGeometry::Cylinder => {
            threed::cylinder_mesh(object.radius, object.height, object.segments)
        }
        notation::model::Scene3dGeometry::Mesh => threed::Mesh {
            vertices: object.vertices.clone(),
            faces: object.faces.clone(),
        },
    };
    threed::Object {
        name: object.name.clone(),
        mesh,
        position: object.position,
        rotation_deg: object.rotation,
        material: threed::Material {
            albedo: [
                f64::from(object.color.r) / 255.0,
                f64::from(object.color.g) / 255.0,
                f64::from(object.color.b) / 255.0,
            ],
            metalness: object.metalness,
            roughness: object.roughness,
        },
    }
}

/// Map one authored light to the [`threed`] core.
pub fn build_light(light: &Scene3dLight) -> threed::Light {
    threed::Light {
        name: light.name.clone(),
        kind: match light.kind {
            Scene3dLightKind::Point => threed::LightKind::Point,
            Scene3dLightKind::Directional => threed::LightKind::Directional,
        },
        position: light.position,
        direction: light.direction,
        color: [
            f64::from(light.color.r) / 255.0,
            f64::from(light.color.g) / 255.0,
            f64::from(light.color.b) / 255.0,
        ],
        intensity: light.intensity,
    }
}

/// Map one authored camera base to the [`threed`] core.
pub fn build_camera_base(camera: &Scene3dCamera) -> threed::CameraBase {
    threed::CameraBase {
        mode: match camera.mode {
            notation::model::Scene3dCameraMode::Orbit => threed::CameraMode::Orbit,
            notation::model::Scene3dCameraMode::Fixed => threed::CameraMode::Fixed,
        },
        distance: camera.distance,
        azimuth_deg: camera.azimuth,
        elevation_deg: camera.elevation,
        position: camera.position,
        target: camera.target,
        fov_deg: camera.fov,
    }
}

/// Map one authored camera key to the [`threed`] core.
pub fn build_camera_key(key: &notation::model::Scene3dCameraKey) -> threed::CameraKey {
    threed::CameraKey {
        at: key.at,
        distance: key.distance,
        azimuth_deg: key.azimuth,
        elevation_deg: key.elevation,
        position: key.position,
        target: key.target,
        fov_deg: key.fov,
    }
}

/// Camera at clip-local time `local_t` seconds: the base with no keys,
/// the bracketing key snapshots lerped linearly with keys, clamped
/// past the ends. Deterministic: keys sort by time, so authored order
/// never matters.
pub fn resolve_camera(camera: &Scene3dCamera, local_t: f64) -> threed::Camera {
    let base = build_camera_base(camera);
    let keys: Vec<threed::CameraKey> = camera.keys.iter().map(build_camera_key).collect();
    threed::camera_at(&base, &keys, local_t)
}

/// Raster size of one scene frame: the evaluated target extents, at
/// least one pixel per side, so a degenerate clip still renders
/// deterministically instead of dividing by zero.
pub fn raster_px(width: f64, height: f64) -> (u32, u32) {
    (
        width.max(1.0).round() as u32,
        height.max(1.0).round() as u32,
    )
}

/// Render one scene frame to a decoded image: objects map to the
/// [`threed`] core, the camera resolves at `local_t`, and an empty
/// light rig renders with the named default light (inside
/// [`threed::render`], never darkness). Crate-internal: the decoded
/// image type never leaves the render crate.
pub(crate) fn render_scene(
    doc: usize,
    objects: &[Scene3dObject],
    camera: &threed::Camera,
    lights: &[Scene3dLight],
    width: u32,
    height: u32,
) -> crate::render::DecodedImage {
    let scene = threed::Scene {
        objects: objects.iter().map(build_object).collect(),
        lights: lights.iter().map(build_light).collect(),
    };
    let image = threed::render(&scene, camera, width, height);
    crate::render::DecodedImage {
        path: std::path::PathBuf::from(format!("{SCENE3D_PREFIX}-{doc}")),
        width: image.width,
        height: image.height,
        rgba: image.rgba,
        scaled: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notation::model::{Rgb, Scene3dGeometry};

    fn box_object() -> Scene3dObject {
        Scene3dObject {
            name: "cube".to_string(),
            kind: Scene3dGeometry::Box,
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            size: [2.0, 2.0, 2.0],
            radius: 1.0,
            height: 1.0,
            segments: 16,
            color: Rgb {
                r: 200,
                g: 32,
                b: 32,
            },
            metalness: 0.0,
            roughness: 0.6,
            vertices: Vec::new(),
            faces: Vec::new(),
            line: 1,
            column: 1,
        }
    }

    fn orbit_camera() -> threed::Camera {
        threed::Camera {
            position: [0.0, 1.5, 6.0],
            target: [0.0, 0.0, 0.0],
            fov_deg: 45.0,
        }
    }

    fn center_covered(rgba: &[u8], w: u32, h: u32) -> bool {
        let i = ((h as usize / 2) * w as usize + w as usize / 2) * 4;
        rgba[i + 3] != 0
    }

    #[test]
    fn mapping_renders_the_box_and_stays_deterministic() {
        let objects = vec![box_object()];
        let camera = orbit_camera();
        let a = render_scene(0, &objects, &camera, &[], 96, 72);
        let b = render_scene(0, &objects, &camera, &[], 96, 72);
        assert_eq!(a.width, 96);
        assert_eq!(a.height, 72);
        assert_eq!(a.rgba, b.rgba);
        assert!(center_covered(&a.rgba, a.width, a.height));
    }

    #[test]
    fn empty_rig_matches_the_named_default_light() {
        let objects = vec![box_object()];
        let camera = orbit_camera();
        let fallback = render_scene(0, &objects, &camera, &[], 80, 60);
        let explicit = Scene3dLight {
            name: threed::DEFAULT_LIGHT_NAME.to_string(),
            kind: Scene3dLightKind::Directional,
            position: [0.0, 0.0, 0.0],
            direction: [-0.35, -1.0, -0.55],
            color: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            intensity: 1.0,
            line: 1,
            column: 1,
        };
        let lit = render_scene(0, &objects, &camera, &[explicit], 80, 60);
        assert_eq!(fallback.rgba, lit.rgba);
        assert!(center_covered(
            &fallback.rgba,
            fallback.width,
            fallback.height
        ));
    }

    #[test]
    fn every_primitive_maps_to_covering_geometry() {
        let camera = orbit_camera();
        let kinds = vec![
            Scene3dObject {
                kind: Scene3dGeometry::Sphere,
                radius: 1.5,
                name: "ball".to_string(),
                ..box_object()
            },
            Scene3dObject {
                kind: Scene3dGeometry::Plane,
                size: [4.0, 4.0, 1.0],
                name: "floor".to_string(),
                ..box_object()
            },
            Scene3dObject {
                kind: Scene3dGeometry::Cylinder,
                radius: 1.0,
                height: 2.0,
                name: "tube".to_string(),
                ..box_object()
            },
            Scene3dObject {
                kind: Scene3dGeometry::Mesh,
                vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
                faces: vec![[0, 1, 2]],
                name: "tri".to_string(),
                ..box_object()
            },
        ];
        for object in &kinds {
            let out = render_scene(0, std::slice::from_ref(object), &camera, &[], 64, 48);
            assert!(
                center_covered(&out.rgba, out.width, out.height),
                "kind {:?} covers the center",
                object.kind
            );
        }
    }

    // --- end-to-end: notation clip to composited frame ---------------------

    use crate::frame::{describe, FrameItem};
    use crate::motion_plan::plan_motions;
    use notation::model::{
        Composition, Fit, Fps, Grade, MotionAsserts, Placement, Scene3dCamera, Scene3dCameraMode,
        Scene3dClip, TrackKind, Turn3D, VfxChain,
    };

    fn test_clip() -> Scene3dClip {
        Scene3dClip {
            asserts: MotionAsserts::default(),
            start: 0.0,
            duration: 2.0,
            id: None,
            track: TrackKind::Graphics,
            label: None,
            start_after: None,
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 48.0,
            fit: Fit::Contain,
            placement: Placement::center(),
            order: 0,
            turn: Turn3D::flat(),
            filter_blur: 0.0,
            grade: Grade::default(),
            effects: Vec::new(),
            vfx: VfxChain::default(),
            hidden: false,
            objects: vec![box_object()],
            camera: Scene3dCamera {
                mode: Scene3dCameraMode::Orbit,
                distance: 6.0,
                azimuth: 0.0,
                elevation: 15.0,
                position: [0.0, 0.0, 5.0],
                target: [0.0, 0.0, 0.0],
                fov: 45.0,
                keys: Vec::new(),
            },
            lights: Vec::new(),
            line: 1,
            column: 1,
        }
    }

    fn test_comp(clip: Scene3dClip) -> Composition {
        Composition {
            line: 1,
            column: 1,
            id: None,
            canvas: None,
            name: "t".to_string(),
            width: 64,
            height: 48,
            duration: 2.0,
            fps: Fps::from_int(30),
            background: notation::model::Rgb { r: 0, g: 0, b: 0 },
            version: 1,
            clips: vec![notation::model::Clip::Scene3d(clip)],
            fonts: Vec::new(),
            tweens: Vec::new(),
            sets: Vec::new(),
            keys: Vec::new(),
            marks: Vec::new(),
            inputs: Vec::new(),
            variables: Vec::new(),
            replicates: Vec::new(),
            autoplay: false,
            normalize: Default::default(),
            normalize_loc: None,
            grade: Grade::default(),
            lane_grades: Vec::new(),
            transitions: Vec::new(),
            motion_blur: None,
        }
    }

    fn paint_frame(comp: &Composition, frame: u64) -> Vec<u8> {
        let motions = plan_motions(comp, 30);
        let desc = describe(comp, &motions, frame);
        let mut raster = crate::render::RasterCache::new();
        let mut out = vec![0u8; 64 * 48 * 3];
        crate::render::draw_description(
            &desc,
            &mut raster,
            &[],
            &crate::render::DynLayouts::default(),
            false,
            &[],
            &[],
            &[],
            &[],
            &crate::path_paint::PathCache::new(),
            std::path::Path::new("."),
            &mut out,
        )
        .unwrap();
        out
    }

    fn center_px(frame: &[u8]) -> [u8; 3] {
        let i = (24 * 64 + 32) * 3;
        [frame[i], frame[i + 1], frame[i + 2]]
    }

    #[test]
    fn paint_enters_the_stack_deterministically() {
        let comp = test_comp(test_clip());
        let a = paint_frame(&comp, 0);
        let b = paint_frame(&comp, 0);
        assert_eq!(a, b, "same inputs paint byte-identical frames");
        assert_ne!(center_px(&a), [0, 0, 0], "the box covers the center");
    }

    #[test]
    fn paint_orbit_and_fixed_cameras_frame_differently() {
        let orbit = paint_frame(&test_comp(test_clip()), 0);
        let mut fixed_clip = test_clip();
        fixed_clip.camera.mode = Scene3dCameraMode::Fixed;
        fixed_clip.camera.position = [0.0, 1.5, 6.0];
        let fixed = paint_frame(&test_comp(fixed_clip), 0);
        assert_ne!(center_px(&orbit), [0, 0, 0]);
        assert_ne!(center_px(&fixed), [0, 0, 0]);
        assert_ne!(orbit, fixed, "viewpoints shade differently");
    }

    #[test]
    fn paint_camera_keys_interpolate_between_frames() {
        use notation::model::Scene3dCameraKey;
        let mut clip = test_clip();
        clip.camera.keys = vec![
            Scene3dCameraKey {
                at: 0.0,
                distance: None,
                azimuth: Some(0.0),
                elevation: None,
                position: None,
                target: None,
                fov: None,
                line: 1,
                column: 1,
            },
            Scene3dCameraKey {
                at: 2.0,
                distance: None,
                azimuth: Some(90.0),
                elevation: None,
                position: None,
                target: None,
                fov: None,
                line: 1,
                column: 1,
            },
        ];
        let comp = test_comp(clip);
        let start = paint_frame(&comp, 0);
        let mid = paint_frame(&comp, 30);
        let end = paint_frame(&comp, 45);
        assert_ne!(start, mid, "the orbit moves between frames");
        assert_ne!(mid, end, "the orbit keeps moving");
        assert_eq!(mid, paint_frame(&comp, 30), "seeks are deterministic");
        // The resolved item carries the clip-local instant.
        let motions = plan_motions(&comp, 30);
        let desc = describe(&comp, &motions, 30);
        match &desc.items[..] {
            [FrameItem::Scene3d(s)] => assert!((s.local_t - 1.0).abs() < 1e-9),
            other => panic!("expected one scene item, got {other:?}"),
        }
    }

    #[test]
    fn paint_default_light_matches_the_named_light() {
        let plain = paint_frame(&test_comp(test_clip()), 0);
        let mut clip = test_clip();
        clip.lights = vec![Scene3dLight {
            name: threed::DEFAULT_LIGHT_NAME.to_string(),
            kind: Scene3dLightKind::Directional,
            position: [0.0, 0.0, 0.0],
            direction: [-0.35, -1.0, -0.55],
            color: notation::model::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            intensity: 1.0,
            line: 1,
            column: 1,
        }];
        let lit = paint_frame(&test_comp(clip), 0);
        assert_eq!(plain, lit, "a missing rig paints the named default light");
        assert_ne!(center_px(&plain), [0, 0, 0]);
    }

    #[test]
    fn paint_backface_culled_triangle_leaves_background() {
        let mut clip = test_clip();
        clip.objects = vec![Scene3dObject {
            kind: Scene3dGeometry::Mesh,
            vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
            faces: vec![[0, 2, 1]],
            name: "rear".to_string(),
            ..box_object()
        }];
        let culled = paint_frame(&test_comp(clip), 0);
        let bare = vec![0u8; 64 * 48 * 3];
        assert_eq!(culled, bare, "rear faces cull to the background");
    }

    #[test]
    fn fingerprint_names_the_scene_and_the_instant() {
        use notation::model::Scene3dCameraKey;
        let mut clip = test_clip();
        clip.camera.keys = vec![
            Scene3dCameraKey {
                at: 0.0,
                distance: None,
                azimuth: Some(0.0),
                elevation: None,
                position: None,
                target: None,
                fov: None,
                line: 1,
                column: 1,
            },
            Scene3dCameraKey {
                at: 2.0,
                distance: None,
                azimuth: Some(90.0),
                elevation: None,
                position: None,
                target: None,
                fov: None,
                line: 1,
                column: 1,
            },
        ];
        let comp = test_comp(clip);
        let motions = plan_motions(&comp, 30);
        let a = crate::gpu::fingerprint(&describe(&comp, &motions, 0));
        let b = crate::gpu::fingerprint(&describe(&comp, &motions, 30));
        assert!(
            a.contains("scene3d#0"),
            "fingerprint names the scene item: {a}"
        );
        assert!(
            a.contains("local_t=0"),
            "fingerprint names the instant: {a}"
        );
        assert!(b.contains("local_t=1"), "seeks fingerprint distinctly: {b}");
        assert_eq!(a, crate::gpu::fingerprint(&describe(&comp, &motions, 0)));
    }

    #[test]
    fn paint_point_and_directional_lights_shade_differently() {
        let mut point_clip = test_clip();
        point_clip.lights = vec![Scene3dLight {
            name: "bulb".to_string(),
            kind: Scene3dLightKind::Point,
            position: [3.0, 3.0, 4.0],
            direction: [0.0, 0.0, 0.0],
            color: notation::model::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            intensity: 30.0,
            line: 1,
            column: 1,
        }];
        let mut dir_clip = test_clip();
        dir_clip.lights = vec![Scene3dLight {
            name: "sun".to_string(),
            kind: Scene3dLightKind::Directional,
            position: [0.0, 0.0, 0.0],
            direction: [0.5, -1.0, -0.4],
            color: notation::model::Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            intensity: 1.0,
            line: 1,
            column: 1,
        }];
        let point = paint_frame(&test_comp(point_clip), 0);
        let directional = paint_frame(&test_comp(dir_clip), 0);
        assert_ne!(center_px(&point), [0, 0, 0]);
        assert_ne!(center_px(&directional), [0, 0, 0]);
        assert_ne!(point, directional, "light kinds shade differently");
    }
}
