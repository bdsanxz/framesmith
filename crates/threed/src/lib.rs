//! Native 3D scene core for the Sprint 21 script adapter (CAP-003).
//!
//! Pure Rust, no browser, no JavaScript, no network: authored
//! primitives (`box`, `sphere`, `plane`, `cylinder`) plus embedded
//! triangle meshes render through a deterministic single-threaded CPU
//! rasterizer with a depth buffer, flat per-face PBR-ish shading
//! (diffuse plus a Blinn-Phong specular lobe mixed by `metalness`
//! and `roughness`), and world-space backface culling.
//!
//! Determinism contract: every operation is fixed-order `f64` math
//! with no randomness, no threads, and no iteration over hash maps.
//! Objects paint in list order; equal-depth fragments keep the first
//! object (strict `<` depth wins), so the same scene description
//! always yields byte-identical pixels.
//!
//! Scene convention: right-handed, `+x` right, `+y` up, `+z` toward
//! the viewer. The camera looks from its position at its target; the
//! vertical field of view (`fov_deg`) sets the perspective scale.
//! Euler rotations apply `Rx`, then `Ry`, then `Rz`
//! (precisely `R = Rz * Ry * Rx`).

/// One 3D point or vector.
pub type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V3) -> V3 {
    let len = dot(a, a).sqrt();
    if len > 0.0 && len.is_finite() {
        scale(a, 1.0 / len)
    } else {
        [0.0, 0.0, 0.0]
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp3(a: V3, b: V3, t: f64) -> V3 {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

/// Rotation matrix for Z-degree Euler angles applied `Rx`, then
/// `Ry`, then `Rz` (`R = Rz * Ry * Rx`).
fn euler_matrix(rotation_deg: V3) -> [[f64; 3]; 3] {
    let (sx, cx) = deg_to_rad(rotation_deg[0]).sin_cos();
    let (sy, cy) = deg_to_rad(rotation_deg[1]).sin_cos();
    let (sz, cz) = deg_to_rad(rotation_deg[2]).sin_cos();
    // Rx then Ry then Rz.
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let mul = |a: [[f64; 3]; 3], b: [[f64; 3]; 3]| -> [[f64; 3]; 3] {
        let mut out = [[0.0; 3]; 3];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let mut acc = 0.0;
                for k in 0..3 {
                    acc += a[i][k] * b[k][j];
                }
                *cell = acc;
            }
        }
        out
    };
    mul(mul(rz, ry), rx)
}

fn apply_matrix(m: [[f64; 3]; 3], v: V3) -> V3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// A triangle mesh: `vertices` in local space, `faces` as 0-based
/// triples wound counter-clockwise seen from outside.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<V3>,
    pub faces: Vec<[usize; 3]>,
}

/// Authored `box` primitive centered on the origin: 12 triangles
/// wound outward.
pub fn box_mesh(w: f64, h: f64, d: f64) -> Mesh {
    let x = w / 2.0;
    let y = h / 2.0;
    let z = d / 2.0;
    let vertices = vec![
        [-x, -y, -z],
        [x, -y, -z],
        [x, y, -z],
        [-x, y, -z],
        [-x, -y, z],
        [x, -y, z],
        [x, y, z],
        [-x, y, z],
    ];
    let faces = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [2, 3, 7],
        [2, 7, 6],
        [1, 2, 6],
        [1, 6, 5],
        [0, 4, 7],
        [0, 7, 3],
    ];
    Mesh { vertices, faces }
}

/// Authored `plane` primitive in the `z = 0` plane facing `+z`.
pub fn plane_mesh(w: f64, h: f64) -> Mesh {
    let x = w / 2.0;
    let y = h / 2.0;
    Mesh {
        vertices: vec![[-x, -y, 0.0], [x, -y, 0.0], [x, y, 0.0], [-x, y, 0.0]],
        faces: vec![[0, 1, 2], [0, 2, 3]],
    }
}

/// Authored `sphere` primitive centered on the origin: `segments`
/// longitude slices (at least 3) by `segments / 2` latitude rings (at
/// least 2), wound outward.
pub fn sphere_mesh(radius: f64, segments: u32) -> Mesh {
    let lon = segments.max(3) as usize;
    let lat = (segments.max(3) / 2).max(2) as usize;
    let mut vertices = Vec::with_capacity((lat + 1) * lon);
    for ring in 0..=lat {
        let theta = std::f64::consts::PI * ring as f64 / lat as f64;
        let (st, ct) = theta.sin_cos();
        for slice in 0..lon {
            let phi = 2.0 * std::f64::consts::PI * slice as f64 / lon as f64;
            let (sp, cp) = phi.sin_cos();
            vertices.push([radius * st * cp, radius * ct, radius * st * sp]);
        }
    }
    let mut faces = Vec::with_capacity(lat * lon * 2);
    for ring in 0..lat {
        for slice in 0..lon {
            let a = ring * lon + slice;
            let b = ring * lon + (slice + 1) % lon;
            let c = (ring + 1) * lon + slice;
            let e = (ring + 1) * lon + (slice + 1) % lon;
            faces.push([a, c, b]);
            faces.push([b, c, e]);
        }
    }
    Mesh { vertices, faces }
}

/// Authored `cylinder` primitive along `+y` centered on the origin:
/// `segments` side quads (at least 3) plus triangle-fan caps, wound
/// outward.
pub fn cylinder_mesh(radius: f64, height: f64, segments: u32) -> Mesh {
    let n = segments.max(3) as usize;
    let y = height / 2.0;
    let mut vertices = Vec::with_capacity(2 * n + 2);
    for i in 0..n {
        let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        let (sa, ca) = a.sin_cos();
        vertices.push([radius * ca, -y, radius * sa]);
    }
    for i in 0..n {
        let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
        let (sa, ca) = a.sin_cos();
        vertices.push([radius * ca, y, radius * sa]);
    }
    let bottom = 2 * n;
    let top = 2 * n + 1;
    vertices.push([0.0, -y, 0.0]);
    vertices.push([0.0, y, 0.0]);
    let mut faces = Vec::with_capacity(4 * n);
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push([i, n + i, n + j]);
        faces.push([i, n + j, j]);
        faces.push([bottom, j, i]);
        faces.push([top, n + i, n + j]);
    }
    Mesh { vertices, faces }
}

/// PBR-ish material: albedo `color` (0 to 1 per channel),
/// `metalness` and `roughness` (both 0 to 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub albedo: [f64; 3],
    pub metalness: f64,
    pub roughness: f64,
}

/// One placed scene object.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    /// Authored name, for refusals.
    pub name: String,
    pub mesh: Mesh,
    /// World-space translation.
    pub position: V3,
    /// Euler degrees applied `Rx`, then `Ry`, then `Rz`.
    pub rotation_deg: V3,
    pub material: Material,
}

/// Camera mode: `orbit` circles a target, `fixed` sits at a point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    Orbit,
    Fixed,
}

/// Authored camera base: orbit properties for `orbit`, `position`
/// for `fixed`, shared `target` and vertical `fov_deg`.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraBase {
    pub mode: CameraMode,
    /// Orbit radius for `orbit`.
    pub distance: f64,
    /// Orbit degrees: 0 puts the camera on `+z`.
    pub azimuth_deg: f64,
    /// Orbit degrees above the horizon.
    pub elevation_deg: f64,
    /// Eye point for `fixed`.
    pub position: V3,
    /// Look-at point for both modes.
    pub target: V3,
    /// Vertical field of view in degrees.
    pub fov_deg: f64,
}

/// One camera key at clip-local time `at` seconds: every `Some`
/// property overrides the base for that key; `None` inherits it.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraKey {
    pub at: f64,
    pub distance: Option<f64>,
    pub azimuth_deg: Option<f64>,
    pub elevation_deg: Option<f64>,
    pub position: Option<V3>,
    pub target: Option<V3>,
    pub fov_deg: Option<f64>,
}

/// A resolved camera: eye, look-at, vertical fov.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    pub position: V3,
    pub target: V3,
    pub fov_deg: f64,
}

/// Orbit eye for a target, distance, and azimuth/elevation degrees.
fn orbit_position(target: V3, distance: f64, azimuth_deg: f64, elevation_deg: f64) -> V3 {
    let az = deg_to_rad(azimuth_deg);
    let el = deg_to_rad(elevation_deg);
    add(
        target,
        [
            distance * el.cos() * az.sin(),
            distance * el.sin(),
            distance * el.cos() * az.cos(),
        ],
    )
}

/// One key resolved against the base: orbital deltas apply to the
/// effective (key or base) target, so every key is a full snapshot
/// independent of key order.
fn resolve_key(base: &CameraBase, key: &CameraKey) -> (f64, Camera) {
    let target = key.target.unwrap_or(base.target);
    let position = match base.mode {
        CameraMode::Orbit => orbit_position(
            target,
            key.distance.unwrap_or(base.distance),
            key.azimuth_deg.unwrap_or(base.azimuth_deg),
            key.elevation_deg.unwrap_or(base.elevation_deg),
        ),
        CameraMode::Fixed => key.position.unwrap_or(base.position),
    };
    (
        key.at,
        Camera {
            position,
            target,
            fov_deg: key.fov_deg.unwrap_or(base.fov_deg),
        },
    )
}

/// Camera at clip-local time `t` seconds: the base with no keys, the
/// bracketing snapshots lerped linearly with keys, clamped past the
/// ends. Keys sort by `at`, so authored order never matters.
pub fn camera_at(base: &CameraBase, keys: &[CameraKey], t: f64) -> Camera {
    let base_camera = Camera {
        position: match base.mode {
            CameraMode::Orbit => orbit_position(
                base.target,
                base.distance,
                base.azimuth_deg,
                base.elevation_deg,
            ),
            CameraMode::Fixed => base.position,
        },
        target: base.target,
        fov_deg: base.fov_deg,
    };
    if keys.is_empty() {
        return base_camera;
    }
    let mut snaps: Vec<(f64, Camera)> = keys.iter().map(|k| resolve_key(base, k)).collect();
    snaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if t <= snaps[0].0 {
        return snaps[0].1.clone();
    }
    if t >= snaps[snaps.len() - 1].0 {
        return snaps[snaps.len() - 1].1.clone();
    }
    for pair in snaps.windows(2) {
        let (t0, c0) = &pair[0];
        let (t1, c1) = &pair[1];
        if t >= *t0 && t <= *t1 {
            let span = t1 - t0;
            let f = if span > 0.0 { (t - t0) / span } else { 0.0 };
            return Camera {
                position: lerp3(c0.position, c1.position, f),
                target: lerp3(c0.target, c1.target, f),
                fov_deg: lerp(c0.fov_deg, c1.fov_deg, f),
            };
        }
    }
    snaps[snaps.len() - 1].1.clone()
}

/// Light kind: `point` radiates from a position with
/// `intensity / (1 + d*d)` falloff, `directional` shines along a
/// direction with uniform `intensity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    Point,
    Directional,
}

/// One scene light.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    /// Authored name, for refusals and readiness.
    pub name: String,
    pub kind: LightKind,
    /// Emitting point for `point` lights.
    pub position: V3,
    /// Travel direction for `directional` lights (need not be unit).
    pub direction: V3,
    /// Light color, 0 to 1 per channel.
    pub color: [f64; 3],
    /// Brightness multiplier, 0 or more.
    pub intensity: f64,
}

/// Name of the fallback light a scene without a light rig renders
/// with: never darkness, always this name.
pub const DEFAULT_LIGHT_NAME: &str = "default-key";

/// The named default light: a soft top-front directional light
/// (white, intensity 1) so an unlit scene still shades
/// deterministically.
pub fn default_light() -> Light {
    Light {
        name: DEFAULT_LIGHT_NAME.to_string(),
        kind: LightKind::Directional,
        position: [0.0, 0.0, 0.0],
        direction: [-0.35, -1.0, -0.55],
        color: [1.0, 1.0, 1.0],
        intensity: 1.0,
    }
}

/// One scene: placed objects plus authored lights. An empty light
/// list renders with [`default_light`].
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    pub objects: Vec<Object>,
    pub lights: Vec<Light>,
}

/// Straight-RGBA raster image: row-major, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
fn shade_face(
    normal: V3,
    frag: V3,
    camera_pos: V3,
    albedo: [f64; 3],
    metalness: f64,
    roughness: f64,
    lights: &[Light],
) -> [u8; 4] {
    let view = norm(sub(camera_pos, frag));
    // A small ambient keeps shadow sides off pure black.
    let mut out = [albedo[0] * 0.10, albedo[1] * 0.10, albedo[2] * 0.10];
    let rough = roughness.clamp(0.0, 1.0);
    let metal = metalness.clamp(0.0, 1.0);
    let shininess = 8.0 + (1.0 - rough) * 120.0;
    let f0 = [
        lerp(0.04, albedo[0], metal),
        lerp(0.04, albedo[1], metal),
        lerp(0.04, albedo[2], metal),
    ];
    for light in lights {
        let (dir, atten) = match light.kind {
            LightKind::Directional => (norm(scale(light.direction, -1.0)), light.intensity),
            LightKind::Point => {
                let delta = sub(light.position, frag);
                let dist_sq = dot(delta, delta);
                let dist = dist_sq.sqrt();
                let dir = if dist > 0.0 {
                    scale(delta, 1.0 / dist)
                } else {
                    [0.0, 0.0, 0.0]
                };
                (dir, light.intensity / (1.0 + dist_sq))
            }
        };
        let n_dot_l = dot(normal, dir).max(0.0);
        if n_dot_l <= 0.0 {
            continue;
        }
        for (i, slot) in out.iter_mut().enumerate() {
            let diffuse = albedo[i] * (1.0 - metal) * light.color[i] * n_dot_l * atten;
            let half = norm(add(dir, view));
            let spec_angle = dot(normal, half).max(0.0);
            let spec = f0[i] * spec_angle.powf(shininess) * (1.0 - rough);
            *slot += diffuse + spec * light.color[i] * atten;
        }
    }
    [
        (out[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (out[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (out[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

/// Render one frame: every object transforms to world space,
/// backfaces cull in world space (a face whose outward normal points
/// away from the camera never rasterizes), survivors project through
/// the pinhole and fill behind a depth buffer. Triangles fully or
/// partly behind the near plane (`z <= 0`) drop; the background stays
/// transparent so the composition shows through.
pub fn render(scene: &Scene, camera: &Camera, width: u32, height: u32) -> Image {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let mut rgba = vec![0u8; w * h * 4];
    let mut depth = vec![f64::INFINITY; w * h];
    let forward = norm(sub(camera.target, camera.position));
    // The reference up falls back when the view runs parallel to it
    // (straight-down orbit or fixed views), so poles frame
    // deterministically instead of collapsing to a line.
    let world_up = if forward[1].abs() < 0.99 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let right = norm(cross(forward, world_up));
    let up = cross(right, forward);
    let persp = (h as f64 / 2.0) / deg_to_rad(camera.fov_deg / 2.0).tan().max(1e-9);
    let lights: Vec<Light> = if scene.lights.is_empty() {
        vec![default_light()]
    } else {
        scene.lights.clone()
    };
    for object in &scene.objects {
        let rot = euler_matrix(object.rotation_deg);
        let world: Vec<V3> = object
            .mesh
            .vertices
            .iter()
            .map(|v| add(apply_matrix(rot, *v), object.position))
            .collect();
        for face in &object.mesh.faces {
            let (ia, ib, ic) = (face[0], face[1], face[2]);
            if ia >= world.len() || ib >= world.len() || ic >= world.len() {
                continue;
            }
            let (a, b, c) = (world[ia], world[ib], world[ic]);
            let raw = cross(sub(b, a), sub(c, a));
            // Degenerate faces (zero area) never shade.
            if !raw[0].is_finite() || !raw[1].is_finite() || !raw[2].is_finite() {
                continue;
            }
            if dot(raw, raw) <= 0.0 {
                continue;
            }
            let normal = apply_matrix(rot, norm(raw));
            let center = scale(add(add(a, b), c), 1.0 / 3.0);
            // Backface culling: faces pointing away from the camera
            // never rasterize.
            if dot(normal, sub(camera.position, center)) <= 0.0 {
                continue;
            }
            let to_cam = |p: V3| -> V3 {
                let d = sub(p, camera.position);
                [dot(d, right), dot(d, up), dot(d, forward)]
            };
            let (pa, pb, pc) = (to_cam(a), to_cam(b), to_cam(c));
            if pa[2] <= 1e-9 || pb[2] <= 1e-9 || pc[2] <= 1e-9 {
                continue;
            }
            let project = |p: V3| -> (f64, f64) {
                (
                    w as f64 / 2.0 + p[0] / p[2] * persp,
                    h as f64 / 2.0 - p[1] / p[2] * persp,
                )
            };
            let (ax, ay) = project(pa);
            let (bx, by) = project(pb);
            let (cx, cy) = project(pc);
            let area = (bx - ax) * (cy - ay) - (cx - ax) * (by - ay);
            if area.abs() < 1e-12 {
                continue;
            }
            let color = shade_face(
                normal,
                center,
                camera.position,
                object.material.albedo,
                object.material.metalness,
                object.material.roughness,
                &lights,
            );
            let x0 = (ax.min(bx).min(cx).floor() as i64).max(0).min(w as i64);
            let x1 = (ax.max(bx).max(cx).ceil() as i64).max(0).min(w as i64);
            let y0 = (ay.min(by).min(cy).floor() as i64).max(0).min(h as i64);
            let y1 = (ay.max(by).max(cy).ceil() as i64).max(0).min(h as i64);
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = x as f64 + 0.5;
                    let py = y as f64 + 0.5;
                    let w0 = (bx - px) * (cy - py) - (cx - px) * (by - py);
                    let w1 = (cx - px) * (ay - py) - (ax - px) * (cy - py);
                    let w2 = (ax - px) * (by - py) - (bx - px) * (ay - py);
                    let inside = if area > 0.0 {
                        w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
                    } else {
                        w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
                    };
                    if !inside {
                        continue;
                    }
                    let denom = 1.0 / area;
                    let z = (w0 * pa[2] + w1 * pb[2] + w2 * pc[2]) * denom;
                    let idx = y as usize * w + x as usize;
                    if z < depth[idx] {
                        depth[idx] = z;
                        let dst = idx * 4;
                        rgba[dst] = color[0];
                        rgba[dst + 1] = color[1];
                        rgba[dst + 2] = color[2];
                        rgba[dst + 3] = color[3];
                    }
                }
            }
        }
    }
    Image {
        width: w as u32,
        height: h as u32,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_box() -> Object {
        Object {
            name: "box".to_string(),
            mesh: box_mesh(2.0, 2.0, 2.0),
            position: [0.0, 0.0, 0.0],
            rotation_deg: [0.0, 0.0, 0.0],
            material: Material {
                albedo: [0.8, 0.1, 0.1],
                metalness: 0.0,
                roughness: 0.6,
            },
        }
    }

    fn orbit_base() -> CameraBase {
        CameraBase {
            mode: CameraMode::Orbit,
            distance: 6.0,
            azimuth_deg: 0.0,
            elevation_deg: 15.0,
            position: [0.0, 0.0, 0.0],
            target: [0.0, 0.0, 0.0],
            fov_deg: 45.0,
        }
    }

    fn fixed_base() -> CameraBase {
        CameraBase {
            mode: CameraMode::Fixed,
            distance: 6.0,
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
            position: [0.0, 1.0, 6.0],
            target: [0.0, 0.0, 0.0],
            fov_deg: 45.0,
        }
    }

    fn center_px(img: &Image) -> [u8; 4] {
        let x = img.width as usize / 2;
        let y = img.height as usize / 2;
        let i = (y * img.width as usize + x) * 4;
        [
            img.rgba[i],
            img.rgba[i + 1],
            img.rgba[i + 2],
            img.rgba[i + 3],
        ]
    }

    fn covered_pixels(img: &Image) -> usize {
        img.rgba.chunks_exact(4).filter(|px| px[3] != 0).count()
    }

    #[test]
    fn orbit_and_fixed_cameras_frame_the_origin() {
        let scene = Scene {
            objects: vec![red_box()],
            lights: vec![],
        };
        let orbit = camera_at(&orbit_base(), &[], 0.0);
        let fixed = camera_at(&fixed_base(), &[], 0.0);
        let a = render(&scene, &orbit, 96, 72);
        let b = render(&scene, &fixed, 96, 72);
        // Both modes put the box over the frame center.
        assert_ne!(center_px(&a), [0, 0, 0, 0]);
        assert_ne!(center_px(&b), [0, 0, 0, 0]);
        // Different viewpoints shade differently.
        assert_ne!(a.rgba, b.rgba);
    }

    #[test]
    fn point_and_directional_lights_shade_differently() {
        let scene = Scene {
            objects: vec![red_box()],
            lights: vec![],
        };
        let camera = camera_at(&orbit_base(), &[], 0.0);
        let point = Scene {
            lights: vec![Light {
                name: "bulb".to_string(),
                kind: LightKind::Point,
                position: [3.0, 3.0, 4.0],
                direction: [0.0, 0.0, 0.0],
                color: [1.0, 1.0, 1.0],
                intensity: 8.0,
            }],
            ..scene.clone()
        };
        let directional = Scene {
            lights: vec![Light {
                name: "sun".to_string(),
                kind: LightKind::Directional,
                position: [0.0, 0.0, 0.0],
                direction: [-0.3, -1.0, -0.4],
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            }],
            ..scene.clone()
        };
        let a = render(&point, &camera, 96, 72);
        let b = render(&directional, &camera, 96, 72);
        assert!(covered_pixels(&a) > 100);
        assert!(covered_pixels(&b) > 100);
        assert_ne!(a.rgba, b.rgba);
    }

    #[test]
    fn backface_culling_hides_rear_faces() {
        // One triangle facing the camera shades; the same triangle
        // wound the other way culls to the background.
        let facing = Mesh {
            vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
            faces: vec![[0, 1, 2]],
        };
        let away = Mesh {
            vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
            faces: vec![[0, 2, 1]],
        };
        let material = Material {
            albedo: [0.2, 0.7, 0.2],
            metalness: 0.0,
            roughness: 0.6,
        };
        let camera = Camera {
            position: [0.0, 0.0, 5.0],
            target: [0.0, 0.0, 0.0],
            fov_deg: 45.0,
        };
        let mk = |mesh: Mesh| Scene {
            objects: vec![Object {
                name: "tri".to_string(),
                mesh,
                position: [0.0, 0.0, 0.0],
                rotation_deg: [0.0, 0.0, 0.0],
                material: material.clone(),
            }],
            lights: vec![Light {
                name: "sun".to_string(),
                kind: LightKind::Directional,
                position: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, -1.0],
                color: [1.0, 1.0, 1.0],
                intensity: 1.0,
            }],
        };
        let lit = render(&mk(facing), &camera, 64, 48);
        let culled = render(&mk(away), &camera, 64, 48);
        assert_ne!(center_px(&lit), [0, 0, 0, 0]);
        assert_eq!(center_px(&culled), [0, 0, 0, 0]);
        assert_eq!(covered_pixels(&culled), 0);
    }

    #[test]
    fn camera_keys_interpolate_deterministically() {
        let base = orbit_base();
        let keys = vec![
            CameraKey {
                at: 0.0,
                distance: None,
                azimuth_deg: Some(0.0),
                elevation_deg: None,
                position: None,
                target: None,
                fov_deg: None,
            },
            CameraKey {
                at: 2.0,
                distance: None,
                azimuth_deg: Some(90.0),
                elevation_deg: None,
                position: None,
                target: None,
                fov_deg: None,
            },
        ];
        let start = camera_at(&base, &keys, 0.0);
        let mid = camera_at(&base, &keys, 1.0);
        let end = camera_at(&base, &keys, 2.0);
        // The midpoint lerps the endpoints exactly.
        assert_eq!(
            mid.position,
            lerp3(start.position, end.position, 0.5),
            "midpoint must lerp endpoints"
        );
        assert_ne!(start.position, end.position);
        assert_ne!(mid.position, start.position);
        // Past the ends the camera holds, and key order is irrelevant.
        assert_eq!(camera_at(&base, &keys, 9.0).position, end.position);
        let mut reversed = keys.clone();
        reversed.reverse();
        assert_eq!(camera_at(&base, &reversed, 1.0), mid);
        // The same scene at the same key time renders byte-identically.
        let scene = Scene {
            objects: vec![red_box()],
            lights: vec![],
        };
        let a = render(&scene, &mid, 80, 60);
        let b = render(&scene, &camera_at(&base, &keys, 1.0), 80, 60);
        assert_eq!(a, b);
    }

    #[test]
    fn poles_frame_deterministically() {
        // Straight-down views run parallel to the reference up: the
        // fallback frames them instead of collapsing to a line.
        let scene = Scene {
            objects: vec![red_box()],
            lights: vec![],
        };
        let mut base = orbit_base();
        base.elevation_deg = 90.0;
        let camera = camera_at(&base, &[], 0.0);
        let a = render(&scene, &camera, 80, 60);
        let b = render(&scene, &camera, 80, 60);
        assert_eq!(a, b);
        assert!(covered_pixels(&a) > 100);
    }

    #[test]
    fn missing_rig_falls_back_to_the_named_default_light() {
        assert_eq!(DEFAULT_LIGHT_NAME, "default-key");
        let fallback = default_light();
        assert_eq!(fallback.name, DEFAULT_LIGHT_NAME);
        let scene = Scene {
            objects: vec![red_box()],
            lights: vec![],
        };
        let explicit = Scene {
            objects: vec![red_box()],
            lights: vec![default_light()],
        };
        let camera = camera_at(&orbit_base(), &[], 0.0);
        let a = render(&scene, &camera, 80, 60);
        let b = render(&explicit, &camera, 80, 60);
        assert_eq!(a, b);
        assert!(covered_pixels(&a) > 100);
    }
}
