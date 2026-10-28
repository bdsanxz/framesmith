//! Where a moving box is drawn, frame by frame (`docs/spec/render-command.md`, section 4.5): a tween's values on its
//! frames, each property half of the way at the middle of a linear tween, the origin and the order of the parts of the
//! map, a to-only tween with no jump, the later of two overlapping tweens in control, `at` in the parent's time, and
//! neighbours that keep their places. Positions are read from the processor's picture; the parts of the map that a
//! picture shows only roughly are read from the frame's group.

mod common;

use common::project_with;
use draw::{Map, Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "motion-places";

/// A scene of a 400 by 60 canvas at 30 frames a second, white, with `body` in its root.
fn scene(name: &str, body: &str) -> Scene {
    let text = format!(
        "<composition version=\"1\" width=\"400\" height=\"60\" fps=\"30\" duration=\"4\" background=\"#ffffff\">\n\
         {body}\n</composition>\n"
    );
    let folder = project_with(GROUP, name, &text);
    Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"))
}

fn picture(scene: &Scene, number: u64) -> Picture {
    scene.picture(number, &mut CpuRenderer).unwrap()
}

/// The first column of row `y` whose pixel is pure red, if any.
fn first_red(picture: &Picture, y: u32) -> Option<u32> {
    (0..picture.width()).find(|x| picture.pixel(*x, y) == Some([255, 0, 0, 255]))
}

/// The first row of column `x` whose pixel is pure red, if any.
fn first_red_down(picture: &Picture, x: u32) -> Option<u32> {
    (0..picture.height()).find(|y| picture.pixel(x, *y) == Some([255, 0, 0, 255]))
}

/// The map and opacity of the one group of a frame.
fn group(scene: &Scene, number: u64) -> (Map, f64) {
    let frame = scene.frame(number).unwrap();
    let groups: Vec<(Map, f64)> = frame
        .marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Group(group) => Some((group.map, group.opacity)),
            _ => None,
        })
        .collect();
    assert_eq!(groups.len(), 1, "frame {number}: {groups:?}");
    groups[0]
}

fn near(found: f64, wanted: f64) -> bool {
    (found - wanted).abs() < 1e-9
}

const BOX: &str = "left=\"10\" top=\"20\" width=\"20\" height=\"20\" background=\"#ff0000\"";

#[test]
fn a_tween_of_x_holds_the_box_in_place_before_it_goes_half_way_at_its_middle_and_stays_at_its_end() {
    let scene = scene(
        "x-across-a-second",
        &format!("<box {BOX}>\n<tween at=\"1\" duration=\"1\" from-x=\"0\" to-x=\"300\"/>\n</box>"),
    );
    for (number, left) in [(0, 10), (29, 10), (30, 10), (45, 160), (60, 310), (61, 310), (119, 310)] {
        assert_eq!(first_red(&picture(&scene, number), 30), Some(left), "frame {number}");
    }
}

#[test]
fn each_property_is_half_of_the_way_at_the_middle_of_a_linear_tween() {
    // Two seconds from 0: frame 30 is the middle.
    let tweened =
        |name: &str, attributes: &str| scene(name, &format!("<box {BOX}>\n<tween at=\"0\" duration=\"2\" {attributes}/>\n</box>"));
    let y = tweened("half-y", "from-y=\"0\" to-y=\"30\"");
    assert_eq!(first_red_down(&picture(&y, 30), 20), Some(35));
    assert!(near(group(&y, 30).0.f, 15.0));

    // The origin is the middle of the box, (20, 30).
    let scale = tweened("half-scale", "from-scale=\"1\" to-scale=\"2\"");
    let (map, _) = group(&scale, 30);
    assert!(
        near(map.a, 1.5) && near(map.d, 1.5) && near(map.b, 0.0) && near(map.c, 0.0),
        "{map:?}"
    );
    assert_eq!(first_red(&picture(&scale, 30), 30), Some(5));

    let across = tweened("half-scale-x", "from-scale-x=\"1\" to-scale-x=\"3\"");
    let (map, _) = group(&across, 30);
    assert!(near(map.a, 2.0) && near(map.d, 1.0), "{map:?}");
    assert_eq!(first_red(&picture(&across, 30), 30), Some(0));

    let turn = tweened("half-rotation", "from-rotation=\"0\" to-rotation=\"90\"");
    let (map, _) = group(&turn, 30);
    let half = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        near(map.a, half) && near(map.b, half) && near(map.c, -half) && near(map.d, half),
        "{map:?}"
    );

    let slant = tweened("half-skew-y", "from-skew-y=\"0\" to-skew-y=\"40\"");
    let (map, _) = group(&slant, 30);
    assert!(
        near(map.b, 20f64.to_radians().tan()) && near(map.a, 1.0) && near(map.d, 1.0),
        "{map:?}"
    );

    let fade = tweened("half-opacity", "from-opacity=\"0\" to-opacity=\"1\"");
    assert!(near(group(&fade, 30).1, 0.5));
    // Red at half is (128, 0, 0, 128) once rounded, a half up; over white, green and blue are 255 less 128: 127.
    assert_eq!(picture(&fade, 30).pixel(20, 30), Some([255, 127, 127, 255]));
}

#[test]
fn scale_slant_and_turn_happen_about_the_origin_in_that_order_before_the_move() {
    let scene = scene(
        "order-and-origin",
        &format!("<box {BOX} transform-origin=\"0 0\" x=\"7\" y=\"3\" scale=\"2\" scale-x=\"1.5\" skew-y=\"10\" rotation=\"30\"/>"),
    );
    let (map, _) = group(&scene, 0);
    let (sin, cos) = 30f64.to_radians().sin_cos();
    let tan = 10f64.to_radians().tan();
    // Each part as a map about the origin (10, 20), the box's top left corner.
    let scale = Map {
        a: 3.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        e: 0.0,
        f: 0.0,
    };
    let slant = Map {
        a: 1.0,
        b: tan,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    let turn = Map {
        a: cos,
        b: sin,
        c: -sin,
        d: cos,
        e: 0.0,
        f: 0.0,
    };
    let wanted = Map::moving(7.0, 3.0)
        .then_after(Map::moving(10.0, 20.0))
        .then_after(turn)
        .then_after(slant)
        .then_after(scale)
        .then_after(Map::moving(-10.0, -20.0));
    for (found, wanted) in [
        (map.a, wanted.a),
        (map.b, wanted.b),
        (map.c, wanted.c),
        (map.d, wanted.d),
        (map.e, wanted.e),
        (map.f, wanted.f),
    ] {
        assert!(near(found, wanted), "{map:?} against {wanted:?}");
    }
    // The origin itself is only moved.
    let (x, y) = map.apply(10.0, 20.0);
    assert!(near(x, 17.0) && near(y, 23.0));

    // Scaled about its top left corner, the box grows to the right and down only.
    let corner = self::scene("origin-corner", &format!("<box {BOX} transform-origin=\"0 0\" scale=\"2\"/>"));
    let drawn = picture(&corner, 0);
    assert_eq!(first_red(&drawn, 30), Some(10));
    assert_eq!(first_red_down(&drawn, 20), Some(20));
    assert_eq!(drawn.pixel(49, 59), Some([255, 0, 0, 255]));
    assert_eq!(drawn.pixel(50, 30), Some([255, 255, 255, 255]));
}

#[test]
fn a_to_only_tween_starts_from_the_value_in_effect_with_no_jump() {
    let scene = scene(
        "to-only",
        &format!("<box {BOX} x=\"50\">\n<tween at=\"1\" duration=\"1\" to-x=\"110\"/>\n</box>"),
    );
    for (number, left) in [(0, 60), (30, 60), (31, 62), (45, 90), (60, 120), (90, 120)] {
        assert_eq!(first_red(&picture(&scene, number), 30), Some(left), "frame {number}");
    }
}

#[test]
fn the_later_of_two_overlapping_tweens_controls_from_its_at_and_cuts_the_earlier_off() {
    let scene = scene(
        "overlap",
        &format!("<box {BOX}>\n<tween at=\"0\" duration=\"2\" to-x=\"200\"/>\n<tween at=\"1\" duration=\"1\" to-x=\"0\"/>\n</box>"),
    );
    // The first goes 100 pixels a second; the second takes over at 1 s from 100 and goes back to 0 by 2 s, and the
    // first, cut off, never reaches 200.
    for (number, left) in [(15, 60), (30, 110), (45, 60), (60, 10), (90, 10)] {
        assert_eq!(first_red(&picture(&scene, number), 30), Some(left), "frame {number}");
    }
}

#[test]
fn at_counts_in_the_parent_time_so_a_tween_at_the_start_of_an_element_begins_as_it_appears() {
    let scene = scene(
        "at-in-the-parent-time",
        &format!("<box {BOX} start=\"1\" duration=\"2\">\n<tween at=\"1\" duration=\"1\" to-x=\"300\"/>\n</box>"),
    );
    assert_eq!(first_red(&picture(&scene, 29), 30), None);
    for (number, left) in [(30, 10), (45, 160), (60, 310), (89, 310)] {
        assert_eq!(first_red(&picture(&scene, number), 30), Some(left), "frame {number}");
    }
    assert_eq!(first_red(&picture(&scene, 90), 30), None);
}

#[test]
fn moving_a_box_leaves_its_neighbours_where_they_were() {
    let row = |name: &str, moved: &str| {
        scene(
            name,
            &format!(
                "<box left=\"0\" top=\"0\" width=\"400\" height=\"60\" flex-direction=\"row\" gap=\"10\">\n\
                 <box width=\"40\" height=\"20\" background=\"#0000ff\"/>\n\
                 <box width=\"40\" height=\"20\" background=\"#ff0000\" {moved}/>\n\
                 <box width=\"40\" height=\"20\" background=\"#00ff00\"/>\n\
                 </box>"
            ),
        )
    };
    let still = picture(&row("row-still", ""), 0);
    let moving = picture(&row("row-moving", "x=\"30\" y=\"25\" scale=\"0.5\" rotation=\"45\""), 0);
    for (x, y) in [(0, 0), (39, 19), (100, 0), (139, 19)] {
        assert_eq!(moving.pixel(x, y), still.pixel(x, y), "({x}, {y})");
    }
    assert_eq!(still.pixel(100, 10), Some([0, 255, 0, 255]));
    assert_eq!(moving.pixel(100, 10), Some([0, 255, 0, 255]));
}
