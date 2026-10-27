//! Shadows of a box and shadows that follow a finished picture (`docs/spec/render-command.md`, section 4.7), drawn by the
//! processor into layers of the frame's size, which the card lays too (rule 6).

use draw::{BoxShadow, Error, Group, Map, Shadow};

use crate::edges::rounded_coverage;
use crate::{blur_alpha, divide_by_255, Coverage};

/// The map that moves the plane of an element by `across` and `down` of its own pixels before `map`.
fn moved_first(map: Map, across: f32, down: f32) -> Map {
    map.then_after(Map {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: f64::from(across),
        f: f64::from(down),
    })
}

/// An alpha plane of the frame's size holding `coverage`, 0 elsewhere.
fn plane(coverage: Option<Coverage>, width: u32, height: u32) -> Vec<u8> {
    let mut plane = vec![0_u8; width as usize * height as usize];
    if let Some(coverage) = coverage {
        for row in 0..coverage.height {
            let start = ((coverage.top + row) * width + coverage.left) as usize;
            let from = (row * coverage.width) as usize;
            plane[start..start + coverage.width as usize].copy_from_slice(&coverage.alpha[from..from + coverage.width as usize]);
        }
    }
    plane
}

/// The standard deviation's blur under `map` (rule 2): the written blur times the square root of how much the map scales an area.
fn scaled_blur(blur: f32, map: Map) -> f32 {
    if map.is_identity() {
        blur
    } else {
        (f64::from(blur) * map.area_scale().sqrt()) as f32
    }
}

/// Premultiplied pixels of `colour` at each level of `alpha`.
fn coloured(alpha: &[u8], shadow: &Shadow) -> Vec<u8> {
    let colour = shadow.colour;
    let mut layer = Vec::with_capacity(alpha.len() * 4);
    for &level in alpha {
        let at = u32::from(level);
        layer.extend_from_slice(&[
            divide_by_255(u32::from(colour.red) * at) as u8,
            divide_by_255(u32::from(colour.green) * at) as u8,
            divide_by_255(u32::from(colour.blue) * at) as u8,
            level,
        ]);
    }
    layer
}

/// A box's shadow as a layer of premultiplied pixels of the frame's size (rule 3): the coverage of the box's outer edge moved by
/// the offset in the box's own pixels and mapped, blurred, then weighed by `255 −` the box's own coverage, in the shadow's colour.
pub fn box_shadow_layer(width: u32, height: u32, mark: &BoxShadow, map: Map) -> Result<Vec<u8>, Error> {
    let shadow = &mark.shadow;
    let moved = moved_first(map, shadow.across, shadow.down);
    let mut alpha = plane(rounded_coverage(mark.shape, moved, width, height)?, width, height);
    blur_alpha(&mut alpha, width as usize, height as usize, scaled_blur(shadow.blur, map));
    let own = plane(rounded_coverage(mark.shape, map, width, height)?, width, height);
    for (value, inside) in alpha.iter_mut().zip(own) {
        *value = divide_by_255(u32::from(*value) * (255 - u32::from(inside))) as u8;
    }
    Ok(coloured(&alpha, shadow))
}

/// A group's finished picture with its shadow under it, as one layer of premultiplied pixels of the frame's size (rule 5):
/// `picture` is the finished picture, drawn with the group's map and turn; its alpha is moved by the offset under the map's
/// turn and scale, each distance rounded to the nearest whole pixel, blurred, coloured, and the picture is laid over it.
pub fn shadowed(width: u32, height: u32, picture: Vec<u8>, shadow: &Shadow, map: Map) -> Vec<u8> {
    let (w, h) = (width as i64, height as i64);
    let (across, down) = (f64::from(shadow.across), f64::from(shadow.down));
    let shift_x = (map.a * across + map.c * down + 0.5).floor() as i64;
    let shift_y = (map.b * across + map.d * down + 0.5).floor() as i64;
    let mut alpha = vec![0_u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (from_x, from_y) = (x - shift_x, y - shift_y);
            if (0..w).contains(&from_x) && (0..h).contains(&from_y) {
                alpha[(y * w + x) as usize] = picture[((from_y * w + from_x) * 4 + 3) as usize];
            }
        }
    }
    blur_alpha(&mut alpha, width as usize, height as usize, scaled_blur(shadow.blur, map));
    let mut layer = coloured(&alpha, shadow);
    crate::lay_layer(&mut layer, &picture);
    layer
}

/// The finished picture of `group` under `map`, with the shadow that follows it, before its cut and its opacity.
pub(crate) fn group_with_shadow(width: u32, height: u32, group: &Group, shadow: &Shadow, map: Map) -> Result<Vec<u8>, Error> {
    let mut picture = vec![0_u8; width as usize * height as usize * 4];
    let whole = (0, 0, width, height);
    match group.turn.filter(|turn| !turn.is_none()) {
        Some(turn) => crate::draw_turned(&mut picture, width, height, group, map, 1.0, turn.about(map), whole)?,
        None => crate::draw_marks(&mut picture, width, height, &group.marks, map, whole)?,
    }
    Ok(shadowed(width, height, picture, shadow, map))
}

/// The layer a group with a shadow that follows lays over the frame (rule 5), for the card's renderer (rule 6): the finished
/// picture with its shadow, each value times the group's opacity, rounded to the nearest level, halves up. `map` is the map
/// around the group; the group's own map is put after it. The card cuts it to the group's clip as it lays it.
pub fn shadowed_group_layer(width: u32, height: u32, group: &Group, map: Map) -> Result<Vec<u8>, Error> {
    let Some(shadow) = &group.shadow else {
        return Err(Error::new("a group with no shadow that follows was given a shadow's layer"));
    };
    let opacity = if group.opacity.is_nan() {
        0.0
    } else {
        group.opacity.clamp(0.0, 1.0)
    };
    let mut layer = group_with_shadow(width, height, group, shadow, map.then_after(group.map))?;
    if opacity < 1.0 {
        for value in &mut layer {
            *value = (f64::from(*value) * opacity + 0.5).floor() as u8;
        }
    }
    Ok(layer)
}
