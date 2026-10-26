// A group turned in space on the graphics card (docs/spec/card-renderer.md, section 10.6; docs/spec/render-command.md,
// section 4.5, rule 10). The group's flat picture is in `flat`; one quad over the frame lays the turned picture. For each
// pixel the point of the flat picture drawn at its centre is found by the inverse of the turn, and its value is taken from
// the four pixels around it, as the processor takes it. The numbers of the turn are in `turn`:
//   0, 1, 2: the rows of the inverse of the turn, each with the origin's x, the origin's y and the opacity after them;
//   3: the row that gives P − Z of a point of the flat picture, from the origin;
//   4: the frame's width and height.

@group(0) @binding(0) var flat: texture_2d<f32>;
@group(0) @binding(1) var<uniform> turn: array<vec4<f32>, 5>;

struct Placed {
    @builtin(position) place: vec4<f32>,
}

@vertex
fn place_turn_corner(@location(0) corner: vec2<f32>) -> Placed {
    var placed: Placed;
    placed.place = vec4<f32>(corner, 0.0, 1.0);
    return placed;
}

// The value of one pixel of the flat picture, faded by the opacity and rounded to a whole level as the processor rounds
// it; transparent outside the frame.
fn faded(x: i32, y: i32) -> vec4<f32> {
    let size = turn[4].xy;
    if (x < 0 || y < 0 || f32(x) >= size.x || f32(y) >= size.y) {
        return vec4<f32>(0.0);
    }
    let value = textureLoad(flat, vec2<i32>(x, y), 0);
    return floor(value * 255.0 * turn[2].w + 0.5);
}

@fragment
fn paint_turned(placed: Placed) -> @location(0) vec4<f32> {
    let origin = vec2<f32>(turn[0].w, turn[1].w);
    let s = vec3<f32>(placed.place.xy - origin, 1.0);
    let h = vec3<f32>(dot(turn[0].xyz, s), dot(turn[1].xyz, s), dot(turn[2].xyz, s));
    if (h.z == 0.0) {
        discard;
    }
    let flat_point = h.xy / h.z;
    if (dot(turn[3].xyz, vec3<f32>(flat_point, 1.0)) <= 0.0) {
        discard;
    }
    let p = origin + flat_point - vec2<f32>(0.5, 0.5);
    let corner = floor(p);
    let f = p - corner;
    let x = i32(corner.x);
    let y = i32(corner.y);
    let upper = faded(x, y) * (1.0 - f.x) + faded(x + 1, y) * f.x;
    let lower = faded(x, y + 1) * (1.0 - f.x) + faded(x + 1, y + 1) * f.x;
    let taken = clamp(floor(upper * (1.0 - f.y) + lower * f.y + 0.5), vec4<f32>(0.0), vec4<f32>(255.0)) / 255.0;
    if (taken.a == 0.0) {
        discard;
    }
    return taken;
}
