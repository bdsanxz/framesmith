//! The frames of the fixture `shadows` hold the shadows worked by hand from `docs/spec/render-command.md`, section
//! 4.7, and from the geometry of the composition (`fixtures/shadows/composition.xml`): a hard shadow, a shadow that follows a
//! rounded box, and the drop-shadow of a group that follows each child and leaves the gap clear. The pixels chosen lie
//! beyond the reach of any blur, and every value below comes from the rule and the geometry, not from the product.

mod common;

use common::project_copied_from_fixture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

/// The picture's pixel at (`x`, `y`) as red, green and blue.
fn rgb(picture: &draw::Picture, x: u32, y: u32) -> [u8; 3] {
    let [red, green, blue, alpha] = picture.pixel(x, y).unwrap();
    assert_eq!(alpha, 255, "the frame is opaque at ({x}, {y})");
    [red, green, blue]
}

#[test]
fn the_shadows_of_the_fixture_are_the_worked_values() {
    let folder = project_copied_from_fixture("shadows-worked-by-hand", "frame", "shadows");
    let project = Project::open(&folder).unwrap();
    let scene = Scene::from_project(&project).unwrap_or_else(|reports| panic!("{reports:?}"));
    // The composition is static, so frames 0 and 5 hold the same worked values; both checked-in pictures are held.
    for number in [0, 5] {
        let picture = scene.picture(number, &mut CpuRenderer).unwrap();
        holds_worked_values(&picture);
    }
}

fn holds_worked_values(picture: &draw::Picture) {
    // Box A: 8 to 31 square, shadow 10 across and down: its moved shape covers 18 to 41, outside the box.
    assert_eq!(rgb(&picture, 36, 20), [0, 0, 0], "A's shadow right of the box");
    assert_eq!(rgb(&picture, 20, 36), [0, 0, 0], "A's shadow below the box");
    assert_eq!(rgb(&picture, 40, 40), [0, 0, 0], "A's shadow in the corner of the moved box");
    assert_eq!(rgb(&picture, 42, 40), [255, 255, 255], "beyond the moved box: the white ground");
    assert_eq!(
        rgb(&picture, 20, 20),
        [0xcc, 0x33, 0x33],
        "under A, its own fill: no shadow shows under its box"
    );

    // Box B: 48 to 71, radius 8. Its green shadow moves it 6 across and down, to 54 to 77 and 14 to 37, its corners rounded.
    // The point (74, 34) is 6.4 from the centre (70, 30) of the moved corner, inside the arc, so green shows; (76, 36) is 9.2
    // from it, outside the arc, and nothing else reaches it, so the ground shows.
    assert_eq!(
        rgb(&picture, 74, 34),
        [0, 0xaa, 0],
        "B's green shadow inside the arc of its moved corner"
    );
    assert_eq!(
        rgb(&picture, 76, 36),
        [255, 255, 255],
        "B's shadow outside the arc of its moved corner"
    );
    assert_eq!(rgb(&picture, 44, 30), [255, 255, 255], "left of B, beyond its shadows");
    assert_eq!(rgb(&picture, 60, 20), [0x33, 0x66, 0xcc], "under B, its own fill");
    // The moved red shadow covers 62 to 85 across and 10 to 33 down, so (75, 20) lies in both moved shapes and beyond
    // the box: the first listed shadow, green, lies on top of the second, red.
    assert_eq!(
        rgb(&picture, 75, 20),
        [0, 0xaa, 0],
        "where the two shadows overlap, the first listed is on top"
    );

    // The group at 60 to 89, 40 to 55, with children 60 to 69 and 80 to 89 (both 40 to 49 down). The shadow moves each child
    // 4 across and down: 64 to 73 and 84 to 93, 44 to 53. (70, 50) is under the first moved child and beyond the child: black.
    // (78, 50) is the gap, reached by neither moved child: the ground. (65, 46) is the first child itself: magenta.
    assert_eq!(rgb(&picture, 70, 50), [0, 0, 0], "the shadow of the first child, beyond it");
    assert_eq!(
        rgb(&picture, 78, 50),
        [255, 255, 255],
        "the gap between the children casts no shadow"
    );
    assert_eq!(rgb(&picture, 65, 46), [255, 0, 255], "the first child is drawn over its shadow");

    // Box C: 8 to 31 across, 40 to 55 down, its shadow 6 across and down with a blur of 3: the moved shape covers 14 to
    // 37 and 46 to 61. The blur reaches 3 pixels (span 3, three passes), so (32, 51), five from the nearest moved edge
    // and beyond the box, is full black.
    assert_eq!(
        rgb(&picture, 32, 51),
        [0, 0, 0],
        "C's blurred shadow at full strength, beyond the blur's reach inside"
    );
}
