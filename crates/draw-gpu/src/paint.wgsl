// The layers of a background on the graphics card (docs/spec/card-renderer.md, section 11; docs/spec/render-command.md,
// section 4.6). Each quad is one layer over a box's rectangle. Its numbers start at the vec4 `start` of `layers`:
//   0: the tile's left and top in pixels of the frame, its width and height;
//   1: whether it repeats (1) or not (0), its kind (0 a colour, 1 linear, 2 radial), its number of stops;
//   2: linear: sin, cos and the tile's centre; radial: the centre and the radii;
//   3: linear: half the line's length;
//   then for each stop its place, and its colour premultiplied.
// The colour given is premultiplied, and is laid over the picture by src-over.

@group(0) @binding(0) var<storage, read> layers: array<vec4<f32>>;

struct Placed {
    @builtin(position) place: vec4<f32>,
    @location(0) @interpolate(flat) start: u32,
}

@vertex
fn place_layer_corner(@location(0) corner: vec2<f32>, @location(1) origin: vec2<f32>) -> Placed {
    var placed: Placed;
    placed.place = vec4<f32>(corner, 0.0, 1.0);
    placed.start = u32(origin.x);
    return placed;
}

@fragment
fn paint_layer(placed: Placed) -> @location(0) vec4<f32> {
    let start = placed.start;
    let tile = layers[start];
    let head = layers[start + 1u];
    let shape = layers[start + 2u];
    let half = layers[start + 3u].x;
    let count = u32(head.z);
    // The pixel's centre, from the tile's top left corner.
    var u = placed.place.x - tile.x;
    var v = placed.place.y - tile.y;
    if (head.x > 0.5) {
        u = u - floor(u / tile.z) * tile.z;
        v = v - floor(v / tile.w) * tile.w;
    } else if (u < 0.0 || u >= tile.z || v < 0.0 || v >= tile.w) {
        discard;
    }
    var t = 0.0;
    if (head.y > 1.5) {
        let du = (u - shape.x) / shape.z;
        let dv = (v - shape.y) / shape.w;
        t = shape.z * sqrt(du * du + dv * dv);
    } else if (head.y > 0.5) {
        t = (u - shape.z) * shape.x - (v - shape.w) * shape.y + half;
    }
    let stops = start + 4u;
    if (t < layers[stops].x) {
        return layers[stops + 1u];
    }
    for (var at = 0u; at + 1u < count; at = at + 1u) {
        let p0 = layers[stops + 2u * at].x;
        let p1 = layers[stops + 2u * at + 2u].x;
        if (p0 <= t && t < p1) {
            let c0 = layers[stops + 2u * at + 1u];
            let c1 = layers[stops + 2u * at + 3u];
            return c0 + (c1 - c0) * ((t - p0) / (p1 - p0));
        }
    }
    return layers[stops + 2u * (count - 1u) + 1u];
}
