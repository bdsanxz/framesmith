//! Still pictures on the processor (`docs/spec/still-pictures.md`, section 5): the value at each pixel is a mean of the
//! picture's premultiplied pixels under a tent filter, widened where the picture is drawn smaller than its own size, times
//! the share of the pixel that the picture's mapped rectangle covers, laid over the picture by src-over.

use draw::{Area, Error, Map, StillMark};
use vello_cpu::kurbo::{self, Affine};
use vello_cpu::peniko::Color;
use vello_cpu::{Pixmap, RenderContext, Resources};

use crate::{affine, divide_by_255, Region};

/// The picture of a still in its box as a picture of the frame's size: its premultiplied values times their coverage,
/// transparent elsewhere (`docs/spec/card-renderer.md`, section 12).
pub fn still_layer(width: u32, height: u32, mark: &StillMark, map: Map) -> Result<Vec<u8>, Error> {
    let mut layer = vec![0_u8; width as usize * height as usize * 4];
    lay_still(&mut layer, width, height, mark, map, (0, 0, width, height))?;
    Ok(layer)
}

/// Lays a still in its box over premultiplied pixels of the frame's size, changing only the pixels of `region`.
pub(crate) fn lay_still(bytes: &mut [u8], width: u32, height: u32, mark: &StillMark, map: Map, region: Region) -> Result<(), Error> {
    if !map.is_finite() {
        return Err(Error::new("a picture is moved by a map that is not finite"));
    }
    let (Some(place), Some(shown)) = (mark.place(), mark.shown()) else {
        return Ok(());
    };
    let Some(inverse) = map.inverse() else {
        return Ok(());
    };
    let Some(coverage) = coverage(shown, map, width, height, region)? else {
        return Ok(());
    };
    let still = &mark.still;
    let (across, down) = (f64::from(still.width), f64::from(still.height));
    let reach_across = (across / (place.width * map.a.hypot(map.b))).max(1.0);
    let reach_down = (down / (place.height * map.c.hypot(map.d))).max(1.0);
    let point = |column: u32, row: u32| {
        let (x, y) = inverse.apply(f64::from(column) + 0.5, f64::from(row) + 0.5);
        ((x - place.left) * across / place.width, (y - place.top) * down / place.height)
    };
    // Under a map with no turn or slant, `u` depends on the column alone and `v` on the row alone, so the weights of each
    // column and each row are worked once.
    let straight = map.b == 0.0 && map.c == 0.0;
    let (left, top) = (coverage.left, coverage.top);
    let columns: Vec<Weights> = if straight {
        (0..coverage.width)
            .map(|column| Weights::at(point(left + column, top).0, reach_across, still.width))
            .collect()
    } else {
        Vec::new()
    };
    let rows: Vec<Weights> = if straight {
        (0..coverage.height)
            .map(|row| Weights::at(point(left, top + row).1, reach_down, still.height))
            .collect()
    } else {
        Vec::new()
    };
    for line in 0..coverage.height {
        for column in 0..coverage.width {
            let covered = u32::from(coverage.alpha[(line * coverage.width + column) as usize]);
            if covered == 0 {
                continue;
            }
            let (x, y) = (left + column, top + line);
            let value = if straight {
                mean(still, &columns[column as usize], &rows[line as usize])
            } else {
                let (u, v) = point(x, y);
                mean(
                    still,
                    &Weights::at(u, reach_across, still.width),
                    &Weights::at(v, reach_down, still.height),
                )
            };
            let laid = value.map(|channel| divide_by_255(channel * covered));
            if laid[3] == 0 && laid[..3].iter().all(|&channel| channel == 0) {
                continue;
            }
            let at = (y as usize * width as usize + x as usize) * 4;
            let pixel = &mut bytes[at..at + 4];
            let kept = 255 - laid[3];
            for channel in 0..4 {
                pixel[channel] = (laid[channel] + divide_by_255(u32::from(pixel[channel]) * kept)).min(255) as u8;
            }
        }
    }
    Ok(())
}

/// The weights of the pixels of one direction of the picture at the point `at`: the first pixel that weighs, and the weight
/// of it and of each after it, `t((i + 0.5 - at) / reach)` with `t(z) = max(0, 1 - |z|)`, over the pixels of the picture.
struct Weights {
    first: usize,
    weights: Vec<f64>,
}

impl Weights {
    fn at(at: f64, reach: f64, count: u32) -> Weights {
        let last = i64::from(count) - 1;
        let low = ((at - reach - 0.5).floor() as i64).clamp(0, last);
        let high = ((at + reach - 0.5).ceil() as i64).clamp(0, last);
        let weights: Vec<f64> = (low..=high)
            .map(|index| (1.0 - ((index as f64 + 0.5 - at) / reach).abs()).max(0.0))
            .collect();
        if weights.iter().all(|&weight| weight == 0.0) {
            // No pixel weighs: the point lies a reach or more outside the picture, and the nearest pixel is taken.
            let nearest = ((at - 0.5).round() as i64).clamp(0, last) as usize;
            return Weights {
                first: nearest,
                weights: vec![1.0],
            };
        }
        Weights {
            first: low as usize,
            weights,
        }
    }

    fn total(&self) -> f64 {
        self.weights.iter().sum()
    }
}

/// The weighted mean of the picture's premultiplied pixels, each channel rounded to the nearest whole level, halves up.
fn mean(still: &draw::Still, across: &Weights, down: &Weights) -> [u32; 4] {
    let mut sum = [0.0_f64; 4];
    for (row_offset, &row_weight) in down.weights.iter().enumerate() {
        if row_weight == 0.0 {
            continue;
        }
        let row = down.first + row_offset;
        let mut line = [0.0_f64; 4];
        for (column_offset, &column_weight) in across.weights.iter().enumerate() {
            if column_weight == 0.0 {
                continue;
            }
            let at = (row * still.width as usize + across.first + column_offset) * 4;
            for (value, &pixel) in line.iter_mut().zip(&still.pixels[at..at + 4]) {
                *value += column_weight * f64::from(pixel);
            }
        }
        for (value, part) in sum.iter_mut().zip(line) {
            *value += row_weight * part;
        }
    }
    let total = across.total() * down.total();
    sum.map(|channel| ((channel / total + 0.5).floor()).clamp(0.0, 255.0) as u32)
}

/// The share of each pixel that the mapped rectangle covers, from 0 to 255, in the box of whole pixels that holds it, cut
/// to the frame and to `region`. Under a map with no turn or slant it is the exact area, rounded to the nearest level,
/// halves up; under any other, the rasteriser's.
struct Coverage {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

fn coverage(area: Area, map: Map, width: u32, height: u32, region: Region) -> Result<Option<Coverage>, Error> {
    let corners = [
        map.apply(area.left, area.top),
        map.apply(area.right(), area.top),
        map.apply(area.right(), area.bottom()),
        map.apply(area.left, area.bottom()),
    ];
    let least_x = corners.iter().map(|corner| corner.0).fold(f64::INFINITY, f64::min);
    let most_x = corners.iter().map(|corner| corner.0).fold(f64::NEG_INFINITY, f64::max);
    let least_y = corners.iter().map(|corner| corner.1).fold(f64::INFINITY, f64::min);
    let most_y = corners.iter().map(|corner| corner.1).fold(f64::NEG_INFINITY, f64::max);
    let (region_left, region_top, region_right, region_bottom) = region;
    let left = least_x.floor().max(f64::from(region_left)).max(0.0);
    let top = least_y.floor().max(f64::from(region_top)).max(0.0);
    let right = most_x.ceil().min(f64::from(region_right)).min(f64::from(width));
    let bottom = most_y.ceil().min(f64::from(region_bottom)).min(f64::from(height));
    if left >= right || top >= bottom {
        return Ok(None);
    }
    let (box_width, box_height) = ((right - left) as u32, (bottom - top) as u32);
    if box_width > u32::from(u16::MAX) || box_height > u32::from(u16::MAX) {
        return Err(Error::new("a picture covers more than 65535 pixels in a direction"));
    }
    let alpha = if map.b == 0.0 && map.c == 0.0 {
        let share = |low: f64, high: f64, at: f64| (high.min(at + 1.0) - low.max(at)).max(0.0);
        let mut alpha = Vec::with_capacity(box_width as usize * box_height as usize);
        for row in 0..box_height {
            let y = top + f64::from(row);
            let down = share(least_y, most_y, y);
            for column in 0..box_width {
                let x = left + f64::from(column);
                alpha.push((share(least_x, most_x, x) * down * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8);
            }
        }
        alpha
    } else {
        let mut context = RenderContext::new(box_width as u16, box_height as u16);
        let mut resources = Resources::new();
        context.set_paint(Color::from_rgba8(255, 255, 255, 255));
        context.set_transform(Affine::translate((-left, -top)) * affine(map));
        context.fill_rect(&kurbo::Rect::new(area.left, area.top, area.right(), area.bottom()));
        context.flush();
        let mut pixmap = Pixmap::new(box_width as u16, box_height as u16);
        context.render(&mut pixmap, &mut resources);
        pixmap.data_as_u8_slice().as_chunks::<4>().0.iter().map(|pixel| pixel[3]).collect()
    };
    Ok(Some(Coverage {
        left: left as u32,
        top: top as u32,
        width: box_width,
        height: box_height,
        alpha,
    }))
}
