// A group cut to a rectangle under its parent's map on the graphics card (docs/spec/card-renderer.md, section 10.1;
// docs/spec/render-command.md, section 4.5, rule 11). The group's picture, faded already, is in `group`, the size of the
// frame; `mask` holds the coverage of the mapped rectangle, the processor's, over the box of whole pixels whose top left
// corner is the quad's origin. One quad over that box lays each value of the group's picture times the coverage.

@group(0) @binding(0) var group: texture_2d<f32>;
@group(0) @binding(1) var mask: texture_2d<f32>;

struct Placed {
    @builtin(position) place: vec4<f32>,
    @location(1) @interpolate(flat) origin: vec2<f32>,
}

@vertex
fn place_coverage_corner(
    @location(0) corner: vec2<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) colour: vec4<f32>,
) -> Placed {
    var placed: Placed;
    placed.place = vec4<f32>(corner, 0.0, 1.0);
    placed.origin = origin;
    return placed;
}

@fragment
fn paint_coverage(placed: Placed) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(placed.place.xy));
    let coverage = textureLoad(mask, vec2<i32>(floor(placed.place.xy - placed.origin)), 0).r;
    return textureLoad(group, texel, 0) * coverage;
}
