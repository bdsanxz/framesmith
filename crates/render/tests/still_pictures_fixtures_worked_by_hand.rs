//! The fixture `still-pictures-fitting` (`docs/spec/still-pictures.md`): its pictures hold the colours its notes give, its
//! frames on the processor equal their checked-in pictures to the byte, and those pictures hold values worked from the
//! spec, by hand at the marked corners and, at every pixel, by a model of section 5 written here from the spec alone.

mod common;

use common::{compare, expected, fixtures, numbers_with_a_picture, project_copied_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "still-pictures-fitting";
const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];
const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const YELLOW: [u8; 3] = [255, 255, 0];
const GREY: [u8; 3] = [128, 128, 128];

/// A picture as its notes describe it: straight RGBA.
struct Made {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn marked() -> Made {
    let mut rgba = Vec::new();
    for y in 0..4 {
        for x in 0..8 {
            let colour = match (x, y) {
                (0, 0) => RED,
                (7, 0) => GREEN,
                (0, 3) => BLUE,
                (7, 3) => YELLOW,
                _ => GREY,
            };
            rgba.extend(colour);
            rgba.push(255);
        }
    }
    Made { width: 8, height: 4, rgba }
}

fn quarter(x: u32, y: u32) -> [u8; 3] {
    [[200, 40, 40], [40, 200, 40], [40, 40, 200], [220, 220, 60]][usize::from(y >= 8) * 2 + usize::from(x >= 8)]
}

fn quarters() -> Made {
    let rgba = (0..16)
        .flat_map(|y| (0..16).flat_map(move |x| [quarter(x, y), [255; 3]].concat()[..4].to_vec()))
        .collect();
    Made {
        width: 16,
        height: 16,
        rgba,
    }
}

fn glass() -> Made {
    Made {
        width: 4,
        height: 4,
        rgba: [255, 0, 0, 128].repeat(16),
    }
}

fn read_back(name: &str) -> Made {
    let bytes = std::fs::read(fixtures().join(FIXTURE).join("pictures").join(name)).unwrap();
    let (width, height, rgba) = common::png::decode(&bytes).unwrap();
    Made { width, height, rgba }
}

#[test]
fn the_pictures_of_the_fixture_hold_the_colours_its_notes_give() {
    for (name, made) in [("marked.png", marked()), ("glass.png", glass()), ("quarters.png", quarters())] {
        let read = read_back(name);
        assert_eq!((read.width, read.height), (made.width, made.height), "{name}");
        assert_eq!(read.rgba, made.rgba, "{name}");
    }
}

fn scene() -> Scene {
    let folder = project_copied_from_fixture("still-pictures-worked", FIXTURE, FIXTURE);
    Scene::from_project(&Project::open(&folder).unwrap()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

#[test]
fn the_frames_drawn_on_the_processor_equal_their_checked_in_pictures() {
    let scene = scene();
    let numbers = numbers_with_a_picture(FIXTURE);
    assert_eq!(numbers, vec![0, 9]);
    for number in numbers {
        let drawn = scene.picture(number, &mut CpuRenderer).unwrap();
        if let Err(difference) = compare(number, &drawn, &expected(FIXTURE, number)) {
            panic!("{difference}");
        }
    }
}

fn at(number: u64, x: u32, y: u32) -> [u8; 3] {
    let pixel = expected(FIXTURE, number).pixel(x, y).unwrap();
    [pixel[0], pixel[1], pixel[2]]
}

#[test]
fn the_marked_corners_are_where_each_fit_and_place_puts_them() {
    for number in [0, 9] {
        let wanted: [((u32, u32), [u8; 3]); 26] = [
            // fill: (4, 4) 40 by 40, the picture stretched five times across and ten times down. At the box's corner pixels
            // the point lies in the corner pixel of the picture, with no weight on its neighbour: u = 0.1, v = 0.05.
            ((4, 4), RED),
            ((43, 4), GREEN),
            ((4, 43), BLUE),
            ((43, 43), YELLOW),
            // cover at 50% 50%: (52, 4) 40 by 40, the picture 80 by 40 from x = 32; the box shows its columns 2 to 5 alone,
            // which are grey.
            ((52, 4), GREY),
            ((91, 43), GREY),
            ((71, 24), GREY),
            // cover at 0% 0%: (100, 4), the picture from x = 100; its left corners show, its right ones are beyond the box.
            ((100, 4), RED),
            ((100, 43), BLUE),
            ((139, 4), GREY),
            ((139, 43), GREY),
            // contain at 50% 50%: (4, 52) 40 by 40, the picture 40 by 20 from y = 62; the rest of the box shows the
            // background.
            ((4, 62), RED),
            ((43, 81), YELLOW),
            ((4, 61), BACKGROUND),
            ((4, 82), BACKGROUND),
            // contain at 100% 100%: the picture 40 by 20 at the bottom of (52, 52), from y = 72.
            ((52, 72), RED),
            ((91, 91), YELLOW),
            ((52, 71), BACKGROUND),
            // No size: the picture's own 8 by 4 at (100, 52).
            ((100, 52), RED),
            ((107, 52), GREEN),
            ((100, 55), BLUE),
            ((107, 55), YELLOW),
            ((108, 52), BACKGROUND),
            // The PNG of quarters with only a height of 24: 24 by 24 at (40, 96), each corner in its own quarter.
            ((40, 96), [200, 40, 40]),
            ((63, 96), [40, 200, 40]),
            ((63, 119), [220, 220, 60]),
        ];
        for ((x, y), colour) in wanted {
            assert_eq!(at(number, x, y), colour, "frame {number}, ({x}, {y})");
        }
        // The glass with only a width of 20: 20 by 20 at (122, 74), (128, 0, 0, 128) over white: 128 + 127, 0 + 127.
        for (x, y) in [(122, 74), (141, 93), (130, 84)] {
            assert_eq!(at(number, x, y), [255, 127, 127], "frame {number}, ({x}, {y})");
        }
        for (x, y) in [(121, 84), (142, 84), (130, 73), (130, 94)] {
            assert_eq!(at(number, x, y), [255, 255, 255], "frame {number}, ({x}, {y})");
        }
        // The JPEG of quarters with only a width of 24, at (4, 96): inside each quarter, within the tolerance of frx7's
        // spec for a JPEG, 6 levels.
        for (x, y, source) in [(8, 100, (2, 2)), (23, 100, (13, 2)), (8, 115, (2, 13)), (23, 115, (13, 13))] {
            let got = at(number, x, y);
            let want = quarter(source.0, source.1);
            for channel in 0..3 {
                assert!(
                    got[channel].abs_diff(want[channel]) <= 6,
                    "frame {number}, ({x}, {y}): {got:?} against {want:?}"
                );
            }
        }
    }
}

/// The value of a picture at the frame pixel `(x, y)` of a place `(left, top, width, height)`, under no map, by the words
/// of section 5: a tent weight in each direction, widened by the reach when the picture is drawn smaller, over every pixel
/// of the picture, on its premultiplied pixels, rounded halves up.
fn modelled(made: &Made, place: (f64, f64, f64, f64), x: u32, y: u32) -> [f64; 4] {
    let (left, top, width, height) = place;
    let (across, down) = (f64::from(made.width), f64::from(made.height));
    let u = (f64::from(x) + 0.5 - left) * across / width;
    let v = (f64::from(y) + 0.5 - top) * down / height;
    let reach_u = (across / width).max(1.0);
    let reach_v = (down / height).max(1.0);
    let (mut sum, mut weights) = ([0.0; 4], 0.0);
    for j in 0..made.height {
        for i in 0..made.width {
            let weight =
                (1.0 - ((f64::from(i) + 0.5 - u) / reach_u).abs()).max(0.0) * (1.0 - ((f64::from(j) + 0.5 - v) / reach_v).abs()).max(0.0);
            if weight == 0.0 {
                continue;
            }
            let pixel = &made.rgba[((j * made.width + i) * 4) as usize..][..4];
            let alpha = f64::from(pixel[3]);
            for channel in 0..3 {
                sum[channel] += weight * (f64::from(pixel[channel]) * alpha / 255.0 + 0.5).floor();
            }
            sum[3] += weight * alpha;
            weights += weight;
        }
    }
    sum.map(|value| (value / weights + 0.5).floor())
}

#[test]
fn every_pixel_outside_the_jpeg_holds_the_value_section_five_gives() {
    // The places of the composition's pictures, worked by hand (section 5), with the box each is cut to.
    type Placed = (Made, (f64, f64, f64, f64), (u32, u32, u32, u32));
    let placed: Vec<Placed> = vec![
        (marked(), (4.0, 4.0, 40.0, 40.0), (4, 4, 40, 40)),
        (marked(), (32.0, 4.0, 80.0, 40.0), (52, 4, 40, 40)),
        (marked(), (100.0, 4.0, 80.0, 40.0), (100, 4, 40, 40)),
        (marked(), (4.0, 62.0, 40.0, 20.0), (4, 52, 40, 40)),
        (marked(), (52.0, 72.0, 40.0, 20.0), (52, 52, 40, 40)),
        (marked(), (100.0, 52.0, 8.0, 4.0), (100, 52, 8, 4)),
        (glass(), (122.0, 74.0, 20.0, 20.0), (122, 74, 20, 20)),
        (quarters(), (40.0, 96.0, 24.0, 24.0), (40, 96, 24, 24)),
    ];
    let (width, height) = (160_u32, 120_u32);
    let mut canvas: Vec<[f64; 3]> = vec![BACKGROUND.map(f64::from); (width * height) as usize];
    for y in 64..104 {
        for x in 112..152 {
            canvas[(y * width + x) as usize] = [255.0; 3];
        }
    }
    for (made, place, (left, top, across, down)) in &placed {
        // What is drawn is the place cut to the box; every place here has whole edges.
        let (from_x, to_x) = ((*left).max(place.0 as u32), (left + across).min((place.0 + place.2) as u32));
        let (from_y, to_y) = ((*top).max(place.1 as u32), (top + down).min((place.1 + place.3) as u32));
        for y in from_y..to_y {
            for x in from_x..to_x {
                let value = modelled(made, *place, x, y);
                let below = &mut canvas[(y * width + x) as usize];
                for channel in 0..3 {
                    below[channel] = value[channel] + (below[channel] * (255.0 - value[3]) / 255.0 + 0.5).floor();
                }
            }
        }
    }
    let jpeg = |x: u32, y: u32| (4..28).contains(&x) && (96..120).contains(&y);
    for number in [0, 9] {
        let picture = expected(FIXTURE, number);
        let mut worst = 0.0_f64;
        let mut place = (0, 0, [0_u8; 4], [0.0_f64; 3]);
        for y in 0..height {
            for x in 0..width {
                if jpeg(x, y) {
                    continue;
                }
                let got = picture.pixel(x, y).unwrap();
                let want = canvas[(y * width + x) as usize];
                for channel in 0..3 {
                    let difference = (f64::from(got[channel]) - want[channel]).abs();
                    if difference > worst {
                        worst = difference;
                        place = (x, y, got, want);
                    }
                }
            }
        }
        println!("frame {number}: the largest difference from the model is {worst} levels, at {place:?}");
        assert!(worst <= 1.0, "frame {number}: {worst} levels from the model");
    }
}
