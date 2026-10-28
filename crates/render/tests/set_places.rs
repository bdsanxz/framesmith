//! Where a box with a `<set>` is drawn, frame by frame (`docs/spec/notation.md` sections 8.3 and 8.5;
//! `docs/spec/render-command.md` section 4.5): the value from the frame at `at`, a set written a hair after a frame's time
//! on that frame by the frame grid rule, a to-only tween after a set that starts from the set's value, a set and a tween at
//! the same `at` in their document order, and a set of each of the seven properties. Positions are read from the
//! processor's picture; the parts of the map that a picture shows only roughly are read from the frame's group.

mod common;

use common::project_with;
use draw::{Map, Mark, Picture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const GROUP: &str = "set-places";

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

/// The map and opacity of the one group of a frame, or `None` when the frame draws the box with no group.
fn group(scene: &Scene, number: u64) -> Option<(Map, f64)> {
    let frame = scene.frame(number).unwrap();
    frame.marks.iter().find_map(|mark| match mark {
        Mark::Group(group) => Some((group.map, group.opacity)),
        _ => None,
    })
}

fn near(found: f64, wanted: f64) -> bool {
    (found - wanted).abs() < 1e-9
}

const BOX: &str = "left=\"10\" top=\"20\" width=\"20\" height=\"20\" background=\"#ff0000\"";

fn lefts(scene: &Scene, numbers: &[(u64, u32)]) {
    for &(number, left) in numbers {
        assert_eq!(first_red(&picture(scene, number), 30), Some(left), "frame {number}");
    }
}

#[test]
fn a_set_of_x_at_two_seconds_leaves_the_box_in_place_on_frame_59_and_moves_it_300_pixels_on_frame_60() {
    let scene = scene("x-at-two", &format!("<box {BOX}>\n<set at=\"2\" x=\"300\"/>\n</box>"));
    lefts(&scene, &[(0, 10), (59, 10), (60, 310), (61, 310), (119, 310)]);
}

#[test]
fn a_set_written_a_hair_after_a_frame_time_lands_on_that_frame_and_one_past_the_tolerance_on_the_next() {
    // A thousandth of a frame at 30 frames a second is 1/30000 s; 2.00002 s is within it after frame 60, and 1.99999 s
    // within it before.
    let after = scene("hair-after", &format!("<box {BOX}>\n<set at=\"2.00002\" x=\"300\"/>\n</box>"));
    lefts(&after, &[(59, 10), (60, 310)]);
    let before = scene("hair-before", &format!("<box {BOX}>\n<set at=\"1.99999\" x=\"300\"/>\n</box>"));
    lefts(&before, &[(59, 10), (60, 310)]);
    // 2.001 s is past the tolerance: the value holds from that time, and the first frame at or after it is frame 61.
    let past = scene("past-tolerance", &format!("<box {BOX}>\n<set at=\"2.001\" x=\"300\"/>\n</box>"));
    lefts(&past, &[(60, 10), (61, 310)]);
}

#[test]
fn a_tween_with_only_a_to_value_after_a_set_starts_from_the_set_value() {
    let scene = scene(
        "set-then-to",
        &format!("<box {BOX}>\n<set at=\"1\" x=\"100\"/>\n<tween at=\"2\" duration=\"1\" to-x=\"200\"/>\n</box>"),
    );
    // x is 0, then 100 from 1 s, held until the tween starts at 2 s, then 100 to 200 by 3 s.
    lefts(
        &scene,
        &[(29, 10), (30, 110), (59, 110), (60, 110), (75, 160), (90, 210), (119, 210)],
    );
}

#[test]
fn a_set_and_a_tween_at_the_same_at_keep_their_document_order_and_the_later_controls() {
    // The set is written later: it controls x from 1 s, and the tween it follows is cut off at once.
    let set_later = scene(
        "tween-then-set",
        &format!("<box {BOX}>\n<tween at=\"1\" duration=\"1\" to-x=\"200\"/>\n<set at=\"1\" x=\"50\"/>\n</box>"),
    );
    lefts(&set_later, &[(29, 10), (30, 60), (45, 60), (60, 60), (90, 60)]);
    // The tween is written later: it controls x from 1 s, going from the set's 50 to 250 by 2 s.
    let tween_later = scene(
        "set-then-tween",
        &format!("<box {BOX}>\n<set at=\"1\" x=\"50\"/>\n<tween at=\"1\" duration=\"1\" to-x=\"250\"/>\n</box>"),
    );
    lefts(&tween_later, &[(29, 10), (30, 60), (45, 160), (60, 260), (90, 260)]);
}

#[test]
fn a_set_of_each_of_the_seven_properties_gives_its_value_from_its_frame() {
    let set = |name: &str, attributes: &str| scene(name, &format!("<box {BOX}>\n<set at=\"1\" {attributes}/>\n</box>"));

    let y = set("set-y", "y=\"15\"");
    assert!(group(&y, 29).is_none_or(|(map, _)| near(map.f, 0.0)));
    assert!(near(group(&y, 30).unwrap().0.f, 15.0));

    // The origin is the middle of the box, (20, 30).
    let scale = set("set-scale", "scale=\"2\"");
    let (map, _) = group(&scale, 30).unwrap();
    assert!(near(map.a, 2.0) && near(map.d, 2.0), "{map:?}");
    assert_eq!(first_red(&picture(&scale, 30), 30), Some(0));
    assert_eq!(first_red(&picture(&scale, 29), 30), Some(10));

    let across = set("set-scale-x", "scale-x=\"3\"");
    let (map, _) = group(&across, 30).unwrap();
    assert!(near(map.a, 3.0) && near(map.d, 1.0), "{map:?}");

    let turn = set("set-rotation", "rotation=\"90\"");
    let (map, _) = group(&turn, 30).unwrap();
    assert!(
        near(map.a, 0.0) && near(map.b, 1.0) && near(map.c, -1.0) && near(map.d, 0.0),
        "{map:?}"
    );

    let slant = set("set-skew-y", "skew-y=\"45\"");
    let (map, _) = group(&slant, 30).unwrap();
    assert!(near(map.b, 1.0) && near(map.a, 1.0) && near(map.d, 1.0), "{map:?}");

    let fade = set("set-opacity", "opacity=\"0\"");
    assert_eq!(picture(&fade, 29).pixel(20, 30), Some([255, 0, 0, 255]));
    assert_eq!(
        picture(&fade, 30).pixel(20, 30),
        Some([255, 255, 255, 255]),
        "opacity 0 hides the box"
    );

    let both = set("set-two", "x=\"100\" opacity=\"0.5\"");
    let (map, opacity) = group(&both, 30).unwrap();
    assert!(near(map.e, 100.0) && near(opacity, 0.5), "{map:?} {opacity}");
}

#[test]
fn a_set_counts_its_at_in_the_time_of_the_parent_of_the_element_it_moves() {
    // The inner box appears at 1 s; its set at 1.5 s is in the outer box's time, the same time as its own start.
    let scene = scene(
        "parent-time",
        &format!(
            "<box left=\"0\" top=\"0\" width=\"400\" height=\"60\">\n<box {BOX} start=\"1\" duration=\"3\">\n<set at=\"1.5\" x=\"100\"/>\n</box>\n</box>"
        ),
    );
    lefts(&scene, &[(30, 10), (44, 10), (45, 110)]);
}
