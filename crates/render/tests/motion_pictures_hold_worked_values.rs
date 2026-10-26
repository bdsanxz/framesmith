//! The checked-in pictures of the motion fixtures hold the values worked out from the specs, independently of the
//! renderer (`tests/common/worked.rs`): every pixel outside a text is within 2 levels of the picture worked from the
//! boxes' places, eases, windows, maps and fades, and the ink of a text has the centre and the mass its map gives. A
//! picture that the product made wrong, and that was checked in as it was made, fails here.

mod common;

use common::worked::{
    grid, hold, ink, ink_left_at_rest, ink_of_colours, moved_rect, near, palindrome_centre_across_from_middle, rect, shown, tween, Area,
    Fill, Movement, Worked,
};
use common::{expected, numbers_with_a_picture};

/// The most a pixel outside a text may lie from its worked value, in levels: the rounding of a fade and of coverage.
const TOLERANCE: f64 = 2.0;

fn colour(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// An area around a text's paragraph at rest, `(left, top)` at `size`, holding at most `glyphs` glyphs, moved by a
/// movement and widened by a margin for the edges of its ink.
fn text_area(left: f64, top: f64, size: f64, glyphs: f64, movement: &Movement, margin: f64) -> Area {
    let corners = moved_rect(left, top, glyphs * size, 1.1 * size, movement);
    let xs = corners.iter().map(|c| c.0);
    let ys = corners.iter().map(|c| c.1);
    (
        xs.clone().fold(f64::MAX, f64::min) - margin,
        ys.clone().fold(f64::MAX, f64::min) - margin,
        xs.fold(f64::MIN, f64::max) + margin,
        ys.fold(f64::MIN, f64::max) + margin,
    )
}

/// The picture of `frame-order` worked out for frame `number`, and the movement of its text.
fn frame_order(number: u64) -> (Worked, Movement) {
    let (fps, t) = (30.0, number as f64 / 30.0);
    let mut worked = Worked::new(160, 90, colour(0xffffff));
    // The red box: out-back(1.7) to x 120 over 1.2 s, then set back to x 10 at 1.5 s.
    let red_x = if number as i64 >= grid(1.5, fps) {
        10.0
    } else {
        tween(t, 0.0, 1.2, 0.0, 120.0, "out-back(1.7)")
    };
    worked.fill(rect(4.0 + red_x, 4.0, 16.0, 16.0), colour(0xd03020));
    // The blue box turns to 90 degrees about its middle and fades to 0.3, in-out-sine over 2 s.
    let blue = Movement {
        rotation: tween(t, 0.0, 2.0, 0.0, 90.0, "in-out-sine"),
        ..Movement::still((80.0, 38.0))
    };
    let opacity = tween(t, 0.0, 2.0, 1.0, 0.3, "in-out-sine");
    worked.lay(
        &[Fill {
            corners: moved_rect(70.0, 28.0, 20.0, 20.0, &blue),
            colour: colour(0x2050c0),
        }],
        opacity,
    );
    // The green clip from 0.6667 s for 0.8 s, with the orange box moving down out-cubic from the clip's start.
    if shown(number, fps, 0.6667, 0.8) {
        worked.fill(rect(100.0, 40.0, 50.0, 40.0), colour(0x20a040));
        let down = tween(t, 0.6667, 0.8, 0.0, 24.0, "out-cubic");
        worked.fill(rect(105.0, 42.0 + down, 10.0, 10.0), colour(0xe08020));
    }
    // The purple clip from 1.2 s for 0.8 s, at the half opacity set at 0.4 s.
    if shown(number, fps, 1.2, 0.8) {
        worked.lay(
            &[Fill {
                corners: rect(10.0, 56.0, 24.0, 24.0),
                colour: colour(0x8030a0),
            }],
            0.5,
        );
    }
    // The text moves left by 20 and grows to 1.3 about its middle, in-expo from 0.2 s for 1.6 s.
    let text = Movement {
        x: tween(t, 0.2, 1.6, 0.0, -20.0, "in-expo"),
        scale: tween(t, 0.2, 1.6, 1.0, 1.3, "in-expo"),
        ..Movement::still((40.0 + 2.0 * 16.0, 60.0 + 0.55 * 16.0))
    };
    (worked, text)
}

#[test]
fn the_checked_in_pictures_of_the_order_proof_hold_their_worked_values() {
    let size = 16.0;
    let (worked, still) = frame_order(0);
    let at_rest = ink(
        &expected("frame-order", 0),
        &worked,
        colour(0x202020),
        text_area(40.0, 60.0, size, 4.0, &still, 3.0),
    );
    near(
        "the left edge of the text's ink at frame 0",
        at_rest.extent.0 as f64,
        ink_left_at_rest(40.0, size),
        1.0,
    )
    .unwrap();
    let line = (60.0, 60.0 + 1.1 * size);
    assert!(
        line.0 <= at_rest.extent.1 as f64 && at_rest.extent.3 as f64 <= line.1.ceil(),
        "the text's ink at frame 0, {:?}, lies in its line, from {} to {}",
        at_rest.extent,
        line.0,
        line.1
    );
    // The ink's centre at rest from the middle of the paragraph, the origin the text grows about: across as the font's
    // spec gives it, down as measured from the ink at rest.
    let from_middle = (palindrome_centre_across_from_middle(size), at_rest.centre.1 - (60.0 + 0.55 * size));
    for number in [0, 20, 30, 36, 45, 59] {
        let picture = expected("frame-order", number);
        let (worked, text) = frame_order(number);
        let area = text_area(40.0, 60.0, size, 4.0, &text, 3.0);
        hold(&picture, &worked, &[area], TOLERANCE).unwrap_or_else(|e| panic!("frame {number}: {e}"));
        let inked = ink(&picture, &worked, colour(0x202020), area);
        let centre = (
            at_rest.centre.0 + text.x + (text.scale - 1.0) * from_middle.0,
            at_rest.centre.1 + text.y + (text.scale - 1.0) * from_middle.1,
        );
        near(
            &format!("frame {number}: the text's ink centre across"),
            inked.centre.0,
            centre.0,
            0.3,
        )
        .unwrap();
        near(
            &format!("frame {number}: the text's ink centre down"),
            inked.centre.1,
            centre.1,
            0.3,
        )
        .unwrap();
        near(
            &format!("frame {number}: the text's ink over its ink at rest"),
            inked.mass / at_rest.mass,
            text.area_scale(),
            0.03,
        )
        .unwrap();
    }
}

/// Holds every checked-in picture of a fixture to the picture worked for its frame, with nothing set aside, and checks
/// that the pictures are those of the frames `chosen`.
fn every_picture_holds(fixture: &str, chosen: &[u64], worked: impl Fn(u64) -> Worked) {
    assert_eq!(numbers_with_a_picture(fixture), chosen, "{fixture}: the frames with a picture");
    for &number in chosen {
        hold(&expected(fixture, number), &worked(number), &[], TOLERANCE).unwrap_or_else(|e| panic!("{fixture}, frame {number}: {e}"));
    }
}

#[test]
fn the_checked_in_pictures_of_a_box_moved_across_hold_their_worked_values() {
    every_picture_holds("motion-box-across", &[0, 5, 10, 15, 19], |number| {
        let t = number as f64 / 10.0;
        let mut worked = Worked::new(200, 90, colour(0xffffff));
        // Linear from x 0 to 120 between 0.5 s and 1.5 s.
        worked.fill(
            rect(10.0 + tween(t, 0.5, 1.0, 0.0, 120.0, "linear"), 20.0, 30.0, 30.0),
            colour(0xd03020),
        );
        worked.fill(rect(10.0, 60.0, 30.0, 20.0), colour(0x2050c0));
        worked
    });
}

#[test]
fn the_checked_in_pictures_of_a_box_that_turns_scales_slants_and_fades_hold_their_worked_values() {
    every_picture_holds("motion-box-turn-and-fade", &[0, 5, 10, 15, 19], |number| {
        let t = number as f64 / 10.0;
        let mut worked = Worked::new(200, 90, colour(0xffffff));
        worked.fill(rect(70.0, 30.0, 80.0, 20.0), colour(0x30a060));
        // Everything linear over 2 s, about the middle of the box, (100, 40).
        let movement = Movement {
            scale: tween(t, 0.0, 2.0, 1.0, 1.5, "linear"),
            scale_x: tween(t, 0.0, 2.0, 1.0, 1.2, "linear"),
            rotation: tween(t, 0.0, 2.0, 0.0, 90.0, "linear"),
            skew_y: tween(t, 0.0, 2.0, 0.0, 15.0, "linear"),
            ..Movement::still((100.0, 40.0))
        };
        let opacity = tween(t, 0.0, 2.0, 1.0, 0.5, "linear");
        worked.lay(
            &[Fill {
                corners: moved_rect(80.0, 25.0, 40.0, 30.0, &movement),
                colour: colour(0xd03020),
            }],
            opacity,
        );
        worked
    });
}

/// The eases of the six boxes of `motion-eased` that move across, top to bottom: a tween with no ease is linear.
const EASES: [&str; 6] = ["linear", "linear", "out-cubic", "in-out-sine", "in-expo", "out-back(1.7)"];

/// The picture of `motion-eased` worked for frame `number`, with the six boxes that move across eased by `eases`.
fn eased(number: u64, eases: [&str; 6]) -> Worked {
    let t = number as f64 / 30.0;
    let mut worked = Worked::new(400, 250, colour(0xffffff));
    let boxes = [
        (10.0, 0xd03020),
        (40.0, 0xe08020),
        (70.0, 0x2050c0),
        (100.0, 0x20a040),
        (130.0, 0x8030a0),
        (160.0, 0x808080),
    ];
    for ((top, hex), ease) in boxes.into_iter().zip(eases) {
        worked.fill(rect(10.0 + tween(t, 0.0, 2.0, 0.0, 300.0, ease), top, 20.0, 20.0), colour(hex));
    }
    // The black box grows from a quarter about its middle, (200, 215), in-out-quint from 0.5 s for 1 s.
    let grown = Movement {
        scale: tween(t, 0.5, 1.0, 0.25, 1.0, "in-out-quint"),
        ..Movement::still((200.0, 215.0))
    };
    worked.fill(moved_rect(180.0, 195.0, 40.0, 40.0, &grown), colour(0x000000));
    worked
}

#[test]
fn the_checked_in_pictures_of_boxes_moved_by_eased_tweens_hold_their_worked_values() {
    every_picture_holds("motion-eased", &[0, 15, 30, 35, 45, 60, 89], |number| eased(number, EASES));
}

#[test]
fn a_checked_in_picture_drawn_with_one_ease_for_another_fails_its_hold() {
    // The mutant a product-only hold misses: had the product eased the grey box by out-cubic where out-back(1.7) is
    // written and its picture been checked in as drawn, the pictures would agree with the product and fail here. Each
    // picture where the two eases differ is held to the picture worked with the wrong ease, and fails.
    let mut wrong = EASES;
    wrong[5] = "out-cubic";
    for number in [15, 30, 35, 45] {
        let held = hold(&expected("motion-eased", number), &eased(number, wrong), &[], TOLERANCE);
        assert!(held.is_err(), "frame {number}: the picture holds the values of the wrong ease");
    }
}

#[test]
fn the_checked_in_pictures_of_boxes_moved_by_sets_on_and_beside_frame_times_hold_their_worked_values() {
    let fps = 30.0;
    every_picture_holds("motion-set-at-an-instant", &[0, 29, 30, 31, 59, 60, 89], |number| {
        let mut worked = Worked::new(400, 130, colour(0xffffff));
        // A set moves its box from the frame its time falls on by the grid of frame-time.md.
        for (top, hex, at) in [(10.0, 0xd03020, 2.0), (50.0, 0x2050c0, 1.00002), (90.0, 0x20a040, 1.001)] {
            let x = if number as i64 >= grid(at, fps) { 300.0 } else { 0.0 };
            worked.fill(rect(10.0 + x, top, 30.0, 30.0), colour(hex));
        }
        worked
    });
}

/// The boxes of `motion-set-and-tween` worked for frame `number`.
fn set_and_tween_boxes(number: u64) -> Worked {
    let (fps, t) = (30.0, number as f64 / 30.0);
    let set = number as i64 >= grid(1.0, fps);
    let mut worked = Worked::new(400, 170, colour(0xffffff));
    // Red: set to 100 at 1 s, then a tween from there to 300 between 1.5 s and 2.5 s.
    let red = if set { tween(t, 1.5, 1.0, 100.0, 300.0, "linear") } else { 0.0 };
    worked.fill(rect(10.0 + red, 10.0, 30.0, 30.0), colour(0xd03020));
    // Blue: the set written after the tween at the same time controls x, which holds 60.
    let blue = if set { 60.0 } else { 0.0 };
    worked.fill(rect(10.0 + blue, 50.0, 30.0, 30.0), colour(0x2050c0));
    // Green: the tween written after the set controls x, from the set's 60 to 300 by 2 s.
    let green = if set { tween(t, 1.0, 1.0, 60.0, 300.0, "linear") } else { 0.0 };
    worked.fill(rect(10.0 + green, 90.0, 30.0, 30.0), colour(0x20a040));
    worked
}

#[test]
fn the_checked_in_pictures_of_sets_in_order_with_tweens_and_a_set_text_hold_their_worked_values() {
    let fixture = "motion-set-and-tween";
    let chosen = [0, 29, 30, 45, 60, 75, 89];
    assert_eq!(numbers_with_a_picture(fixture), chosen);
    let (size, line): (f64, (f64, f64)) = (30.0, (128.0, 128.0 + 1.1 * 30.0));
    let rest = expected(fixture, 0);
    let at_rest = ink(
        &rest,
        &set_and_tween_boxes(0),
        colour(0x000000),
        (0.0, line.0, 400.0, line.1.ceil()),
    );
    near(
        "the left edge of the text's ink at rest",
        at_rest.extent.0 as f64,
        ink_left_at_rest(10.0, size),
        1.0,
    )
    .unwrap();
    assert!(at_rest.mass > 50.0, "the text has ink at rest: {at_rest:?}");
    for number in chosen {
        let picture = expected(fixture, number);
        let worked = set_and_tween_boxes(number);
        // The text's line is held apart; everything else is the boxes.
        hold(&picture, &worked, &[(0.0, line.0, 400.0, line.1)], TOLERANCE).unwrap_or_else(|e| panic!("frame {number}: {e}"));
        // From 1 s the text is moved by 200 whole pixels and laid at half opacity: each pixel of its line is the pixel 200
        // to its left at rest, halfway to the background, rounded once.
        let (shift, opacity) = if number >= 30 { (200, 0.5) } else { (0, 1.0) };
        for y in line.0 as u32..line.1.ceil() as u32 {
            for x in 0..400u32 {
                let drawn = picture.pixel(x, y).unwrap();
                let from = if x >= shift { rest.pixel(x - shift, y).unwrap() } else { [255; 4] };
                for channel in 0..3 {
                    let worked = 255.0 - (255.0 - f64::from(from[channel])) * opacity;
                    assert!(
                        (f64::from(drawn[channel]) - worked).abs() <= 1.0,
                        "frame {number}: pixel ({x}, {y}) is {drawn:?}; the text at rest 200 to its left, {from:?}, at \
                         opacity {opacity}, gives {worked:.1} in channel {channel}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_checked_in_pictures_of_a_box_of_boxes_and_text_that_fades_as_one_hold_their_worked_values() {
    let fixture = "motion-group-fade";
    let chosen = [0, 5, 10, 15, 19];
    assert_eq!(numbers_with_a_picture(fixture), chosen);
    let size = 20.0;
    let worked_at = |number: u64| {
        let t = number as f64 / 10.0;
        let (down, opacity) = (tween(t, 0.0, 2.0, 0.0, 30.0, "linear"), tween(t, 0.0, 2.0, 1.0, 0.0, "linear"));
        let mut worked = Worked::new(200, 90, colour(0xf0f0f0));
        // The two boxes are drawn into the group's layer, the blue over the red, and fade with it as one picture.
        worked.lay(
            &[
                Fill {
                    corners: rect(20.0, 10.0 + down, 60.0, 40.0),
                    colour: colour(0xd03020),
                },
                Fill {
                    corners: rect(50.0, 20.0 + down, 60.0, 40.0),
                    colour: colour(0x2050c0),
                },
            ],
            opacity,
        );
        // The text's paragraph at (120, 20 + down), with a margin for its edges.
        (
            worked,
            down,
            opacity,
            (117.0, 17.0 + down, 120.0 + 2.0 * size + 3.0, 20.0 + down + 1.1 * size + 3.0),
        )
    };
    let (worked, _, _, area) = worked_at(0);
    let at_rest = ink(&expected(fixture, 0), &worked, colour(0x000000), area);
    near(
        "the left edge of the text's ink at rest",
        at_rest.extent.0 as f64,
        ink_left_at_rest(120.0, size),
        1.0,
    )
    .unwrap();
    for number in chosen {
        let picture = expected(fixture, number);
        let (worked, down, opacity, area) = worked_at(number);
        hold(&picture, &worked, &[area], TOLERANCE).unwrap_or_else(|e| panic!("frame {number}: {e}"));
        // The text moves down with the group and fades with it: its ink is its ink at rest, moved, times the opacity.
        let inked = ink(&picture, &worked, colour(0x000000), area);
        near(
            &format!("frame {number}: the text's ink over its ink at rest"),
            inked.mass / at_rest.mass,
            opacity,
            0.03,
        )
        .unwrap();
        near(
            &format!("frame {number}: the text's ink centre across"),
            inked.centre.0,
            at_rest.centre.0,
            0.3,
        )
        .unwrap();
        near(
            &format!("frame {number}: the text's ink centre down"),
            inked.centre.1,
            at_rest.centre.1 + down,
            0.3,
        )
        .unwrap();
    }
}

#[test]
fn the_checked_in_pictures_of_a_dressed_text_that_moves_scales_and_turns_hold_their_worked_values() {
    let fixture = "motion-text";
    let chosen = [0, 5, 10, 15, 19];
    assert_eq!(numbers_with_a_picture(fixture), chosen);
    let size = 24.0;
    let blank = Worked::new(200, 90, colour(0xffffff));
    // About the top left corner of the paragraph, (20, 20), linear over 2 s.
    let movement = |number: u64| {
        let t = number as f64 / 10.0;
        Movement {
            x: tween(t, 0.0, 2.0, 0.0, 60.0, "linear"),
            y: tween(t, 0.0, 2.0, 0.0, 10.0, "linear"),
            scale: tween(t, 0.0, 2.0, 1.0, 1.4, "linear"),
            rotation: tween(t, 0.0, 2.0, 0.0, 20.0, "linear"),
            ..Movement::still((20.0, 20.0))
        }
    };
    // The shadow lies 3 pixels right and down, blurred, and the outline 1 pixel wide: the whole ink, every colour, is held.
    let whole = (0.0, 0.0, 200.0, 90.0);
    let at_rest = ink_of_colours(&expected(fixture, 0), &blank, whole);
    let ink_line = ink(&expected(fixture, 0), &blank, colour(0x202020), whole);
    near(
        "the left edge of the text's ink at rest",
        ink_line.extent.0 as f64,
        ink_left_at_rest(20.0, size) - 0.5,
        1.0,
    )
    .unwrap();
    for number in chosen {
        let picture = expected(fixture, number);
        let movement = movement(number);
        // Nothing but the text is drawn: every pixel outside the area its map takes the paragraph to is the background.
        let area = text_area(20.0, 20.0, size, 4.0, &movement, 8.0);
        hold(&picture, &blank, &[area], TOLERANCE).unwrap_or_else(|e| panic!("frame {number}: {e}"));
        let inked = ink_of_colours(&picture, &blank, whole);
        let centre = movement.apply(at_rest.centre);
        // A pass of an even span averages a pixel with the next one, so it moves the shadow half a pixel back; three
        // passes move it one and a half (`text.md`, section 6.6). While the span is the one at rest, that move is in the
        // ink at rest too, and the centre follows the map within 0.4 pixels. Once the span changes, the shadow, less than
        // two thirds of the ink, lies up to one and a half pixels from where the map takes it: the centre within 1 pixel.
        let bound = if shadow_span(movement.area_scale().sqrt()) == shadow_span(1.0) {
            0.4
        } else {
            1.0
        };
        near(&format!("frame {number}: the ink's centre across"), inked.centre.0, centre.0, bound).unwrap();
        near(&format!("frame {number}: the ink's centre down"), inked.centre.1, centre.1, bound).unwrap();
        // The shadow's blur runs over a whole number of pixels, worked from its standard deviation scaled with the map
        // (`text.md`, section 6.6; `render-command.md`, section 4.5, rule 7). While that span is the one at rest, the ink
        // grows with the map's area, within 5 per cent: the spread of the shadow does not grow with it. Once the span
        // grows, the shadow spreads further and the ink grows past the map's area.
        let ratio = inked.mass / at_rest.mass;
        if shadow_span(movement.area_scale().sqrt()) == shadow_span(1.0) {
            near(
                &format!("frame {number}: the ink over the ink at rest"),
                ratio,
                movement.area_scale(),
                0.05 * movement.area_scale(),
            )
            .unwrap();
        } else {
            assert!(
                ratio > movement.area_scale(),
                "frame {number}: the ink over the ink at rest is {ratio:.3}, under the map's area"
            );
        }
    }
}

/// The span in pixels of each pass of the blur of `motion-text`'s shadow, `2px`, under a map that scales lengths by `scale`:
/// a standard deviation of half the blur times the scale (`text.md`, section 6.6).
fn shadow_span(scale: f64) -> i64 {
    let sigma = 2.0 / 2.0 * scale;
    (sigma * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0 + 0.5).floor() as i64
}
