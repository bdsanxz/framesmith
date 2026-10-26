// One pass of the blur and brightness of a group on the graphics card (docs/spec/card-renderer.md, section 10.7;
// docs/spec/render-command.md, section 4.5, rule 11). One quad over the frame writes each pixel of the next texture from
// `picture`: the sum of the pixels along one direction, each times its weight, what lies outside the frame taken as clear.
// The last pass of a group also brightens: each colour value times the brightness, kept at most the alpha, and rounds to a
// whole level as the processor rounds. The numbers of the pass are in `pass`:
//   x: 0 across, 1 down; y: how many pixels the weights reach to each side; z: the brightness; w: 1 when the pass brightens.

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> pass_numbers: vec4<f32>;
@group(0) @binding(2) var<storage, read> weights: array<f32>;

struct Placed {
    @builtin(position) place: vec4<f32>,
}

@vertex
fn place_filter_corner(@location(0) corner: vec2<f32>) -> Placed {
    var placed: Placed;
    placed.place = vec4<f32>(corner, 0.0, 1.0);
    return placed;
}

@fragment
fn paint_filtered(placed: Placed) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(picture));
    let here = vec2<i32>(floor(placed.place.xy));
    var step = vec2<i32>(1, 0);
    if (pass_numbers.x > 0.5) {
        step = vec2<i32>(0, 1);
    }
    let reach = i32(pass_numbers.y);
    var sum = vec4<f32>(0.0);
    for (var k = -reach; k <= reach; k = k + 1) {
        let at = here + step * k;
        if (at.x >= 0 && at.y >= 0 && at.x < size.x && at.y < size.y) {
            sum = sum + weights[u32(k + reach)] * textureLoad(picture, at, 0) * 255.0;
        }
    }
    if (pass_numbers.w > 0.5) {
        let colour = min(sum.rgb * pass_numbers.z, vec3<f32>(sum.a));
        sum = vec4<f32>(colour, sum.a);
    }
    return clamp(floor(sum + 0.5), vec4<f32>(0.0), vec4<f32>(255.0)) / 255.0;
}
