// The fills of a frame on the graphics card (docs/spec/card-renderer.md, section 1). The caller
// places every corner in clip space and gives its colour as a number from 0 to 1; nothing here is
// blended, so the last fill drawn over a pixel is what the pixel holds.

struct Painted {
    @builtin(position) place: vec4<f32>,
    @location(1) @interpolate(flat) colour: vec4<f32>,
}

@vertex
fn place_corner(@location(0) corner: vec2<f32>, @location(2) colour: vec4<f32>) -> Painted {
    var painted: Painted;
    painted.place = vec4<f32>(corner, 0.0, 1.0);
    painted.colour = colour;
    return painted;
}

@fragment
fn paint(painted: Painted) -> @location(0) vec4<f32> {
    return painted.colour;
}
