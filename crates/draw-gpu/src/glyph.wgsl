// The glyphs of a frame on the graphics card (docs/spec/card-renderer.md, section 9.4). A glyph is a quad of whole
// pixels. Each of its pixels takes the coverage of the glyph's mask at the same place, read without filtering, and
// the colour of the run is blended over the picture by that coverage.

@group(0) @binding(0) var mask: texture_2d<f32>;

struct Glyph {
    @builtin(position) place: vec4<f32>,
    @location(1) @interpolate(flat) origin: vec2<f32>,
    @location(2) @interpolate(flat) colour: vec4<f32>,
}

@vertex
fn place_glyph_corner(
    @location(0) corner: vec2<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) colour: vec4<f32>,
) -> Glyph {
    var glyph: Glyph;
    glyph.place = vec4<f32>(corner, 0.0, 1.0);
    glyph.origin = origin;
    glyph.colour = colour;
    return glyph;
}

// The colour, premultiplied by its own alpha (1 for a glyph), is multiplied by the coverage, so that the blend keeps the
// picture where the glyph is clear. A ring of a border is drawn the same way (docs/spec/box-edges.md, section 7).
@fragment
fn paint_glyph(glyph: Glyph) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(glyph.place.xy - glyph.origin));
    let coverage = textureLoad(mask, texel, 0).r;
    return glyph.colour * coverage;
}

// The coverage of a cut's shape in all four channels: the blend multiplies what the cut's texture holds by it, so that nothing
// is kept outside the shape (docs/spec/box-edges.md, section 7).
@fragment
fn keep_inside(glyph: Glyph) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(glyph.place.xy - glyph.origin));
    return vec4<f32>(textureLoad(mask, texel, 0).r);
}

// A colour picture of an emoji (docs/spec/text.md, section 12.4; docs/spec/card-renderer.md, section 9.8): each pixel is read
// as it is stored, premultiplied, without filtering, and blended over the frame by the blend of the glyphs, which keeps the
// pixel's own colours whatever the colour of the text. The texture of a group that fades is drawn the same way, each value
// times the group's opacity, which the colour of the corners carries (section 10.1); a picture's corners carry 1.
@fragment
fn paint_picture(glyph: Glyph) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(glyph.place.xy - glyph.origin));
    return textureLoad(mask, texel, 0) * glyph.colour;
}
