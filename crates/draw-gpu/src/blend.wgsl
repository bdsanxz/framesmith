// A group laid with a blend on the graphics card (docs/spec/blends.md, sections 3, 4 and 6). The group's picture, faded
// already, is in `group`; what was under it, copied before this draw, is in `under`. One quad over the group's cut writes,
// for each pixel, the colour of section 3 in place of what was there. The blend's number (its place in the list of
// draw::Blend::ALL) is the first value of the quad's colour.

@group(0) @binding(0) var group: texture_2d<f32>;
@group(0) @binding(1) var under: texture_2d<f32>;

struct Placed {
    @builtin(position) place: vec4<f32>,
    @location(2) @interpolate(flat) colour: vec4<f32>,
}

@vertex
fn place_blend_corner(
    @location(0) corner: vec2<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) colour: vec4<f32>,
) -> Placed {
    var placed: Placed;
    placed.place = vec4<f32>(corner, 0.0, 1.0);
    placed.colour = colour;
    return placed;
}

fn multiply(b: f32, s: f32) -> f32 {
    return b * s;
}

fn screen(b: f32, s: f32) -> f32 {
    return b + s - b * s;
}

fn hard_light(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return multiply(b, 2.0 * s);
    }
    return screen(b, 2.0 * s - 1.0);
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if (b == 0.0) {
        return 0.0;
    }
    if (s == 1.0) {
        return 1.0;
    }
    return min(1.0, b / (1.0 - s));
}

fn color_burn(b: f32, s: f32) -> f32 {
    if (b == 1.0) {
        return 1.0;
    }
    if (s == 0.0) {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - b) / s);
}

fn soft_light(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    var d = sqrt(b);
    if (b <= 0.25) {
        d = ((16.0 * b - 12.0) * b + 4.0) * b;
    }
    return b + (2.0 * s - 1.0) * (d - b);
}

// The blends that work on each channel alone, by number.
fn separable(blend: i32, b: f32, s: f32) -> f32 {
    switch blend {
        case 1: { return multiply(b, s); }
        case 2: { return screen(b, s); }
        case 3: { return hard_light(s, b); }
        case 4: { return min(b, s); }
        case 5: { return max(b, s); }
        case 6: { return color_dodge(b, s); }
        case 7: { return color_burn(b, s); }
        case 8: { return hard_light(b, s); }
        case 9: { return soft_light(b, s); }
        case 10: { return abs(b - s); }
        case 11: { return b + s - 2.0 * b * s; }
        default: { return s; }
    }
}

fn lum(c: vec3<f32>) -> f32 {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

fn sat(c: vec3<f32>) -> f32 {
    return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}

fn clip_colour(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    var out = c;
    if (n < 0.0) {
        out = l + (out - l) * l / (l - n);
    }
    if (x > 1.0) {
        out = l + (out - l) * (1.0 - l) / (x - l);
    }
    return out;
}

fn with_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_colour(c + (l - lum(c)));
}

// The channels in a stable order of their values, as the processor orders them: the least, the middle and the greatest.
fn with_sat(c: vec3<f32>, t: f32) -> vec3<f32> {
    var order = vec3<i32>(0, 1, 2);
    if (c[order.y] < c[order.x]) {
        order = vec3<i32>(order.y, order.x, order.z);
    }
    if (c[order.z] < c[order.y]) {
        order = vec3<i32>(order.x, order.z, order.y);
    }
    if (c[order.y] < c[order.x]) {
        order = vec3<i32>(order.y, order.x, order.z);
    }
    var out = vec3<f32>(0.0);
    let least = c[order.x];
    let most = c[order.z];
    if (most > least) {
        out[order.y] = (c[order.y] - least) * t / (most - least);
        out[order.z] = t;
    }
    return out;
}

fn mixed(blend: i32, b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    switch blend {
        case 12: { return with_lum(with_sat(s, sat(b)), lum(b)); }
        case 13: { return with_lum(with_sat(b, sat(s)), lum(b)); }
        case 14: { return with_lum(s, lum(b)); }
        case 15: { return with_lum(b, lum(s)); }
        default: {
            return vec3<f32>(separable(blend, b.r, s.r), separable(blend, b.g, s.g), separable(blend, b.b, s.b));
        }
    }
}

fn unmultiplied(c: vec4<f32>) -> vec3<f32> {
    if (c.a == 0.0) {
        return vec3<f32>(0.0);
    }
    return min(c.rgb / c.a, vec3<f32>(1.0));
}

@fragment
fn paint_blended(placed: Placed) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(placed.place.xy));
    // Whole levels, as the processor reads them.
    let cs = floor(textureLoad(group, texel, 0) * 255.0 + 0.5) / 255.0;
    let cb = floor(textureLoad(under, texel, 0) * 255.0 + 0.5) / 255.0;
    if (cs.a == 0.0) {
        return cb;
    }
    let b = mixed(i32(placed.colour.r + 0.5), unmultiplied(cb), unmultiplied(cs));
    let colour = cs.rgb * (1.0 - cb.a) + cb.rgb * (1.0 - cs.a) + cs.a * cb.a * b;
    let alpha = cs.a + cb.a - cs.a * cb.a;
    return clamp(floor(vec4<f32>(colour, alpha) * 255.0 + 0.5) / 255.0, vec4<f32>(0.0), vec4<f32>(1.0));
}
