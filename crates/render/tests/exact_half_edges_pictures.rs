//! A percentage is worked out exactly, and a half rounds up, in the boxes of the exact-halves fixture: each box covers the
//! whole-pixel columns that `docs/spec/render-command.md` section 4.1, rule 3 gives, on the processor, and the frames equal
//! their checked-in pictures to the byte. The columns are worked by hand in the comments of the fixture.

mod common;

use common::{compare, count_of, expected, extent_of, numbers_with_a_picture, project_from_fixture};
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "placement-exact-halves";
const ORANGE: [u8; 4] = [0xe0, 0xa0, 0x30, 0xff];
const TEAL: [u8; 4] = [0x30, 0xc0, 0xa0, 0xff];
const PURPLE: [u8; 4] = [0xa0, 0x50, 0xd0, 0xff];
const RED: [u8; 4] = [0xd0, 0x50, 0x30, 0xff];
const BACKGROUND: [u8; 4] = [0x10, 0x20, 0x40, 0xff];

fn drawn_frame(number: u64) -> draw::Picture {
    let folder = project_from_fixture("exact-half-edges-pictures", FIXTURE, FIXTURE);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(number, &mut CpuRenderer).unwrap()
}

#[test]
fn the_frames_equal_their_checked_in_pictures() {
    assert_eq!(
        numbers_with_a_picture(FIXTURE),
        vec![0, 9],
        "the first and the last frame have a picture"
    );
    for number in [0, 9] {
        if let Err(difference) = compare(number, &drawn_frame(number), &expected(FIXTURE, number)) {
            panic!("{FIXTURE}: {difference}");
        }
    }
}

#[test]
fn a_box_starting_at_one_point_four_percent_covers_columns_39_to_48() {
    // 1.4% of 2750 is exactly 38.5: the edges lie at 38.5 and 48.5, which round half up to 39 and 49.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, ORANGE), Some((39, 0, 49, 12)));
    assert_eq!(count_of(&frame, ORANGE), 120);
    assert_eq!(frame.pixel(38, 0), Some(TEAL));
    assert_eq!(frame.pixel(39, 0), Some(ORANGE));
    assert_eq!(frame.pixel(48, 0), Some(ORANGE));
    assert_eq!(frame.pixel(49, 0), Some(BACKGROUND));
}

#[test]
fn a_box_sized_one_point_four_percent_from_the_near_edge_covers_columns_0_to_38() {
    // The size is 1.4% of 2750, exactly 38.5, from the edge at 0: the far edge is 38.5, which rounds up to 39.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, TEAL), Some((0, 0, 39, 12)));
    assert_eq!(count_of(&frame, TEAL), 468);
}

#[test]
fn a_box_starting_at_four_point_six_percent_covers_columns_127_to_136() {
    // 4.6% of 2750 is exactly 126.5, which rounds half up to 127; the far edge is 136.5, which rounds to 137.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, PURPLE), Some((127, 0, 137, 12)));
    assert_eq!(count_of(&frame, PURPLE), 120);
}

#[test]
fn a_box_placed_from_the_far_edge_covers_columns_2702_to_2711() {
    // 1.4% of 2750 is 38.5 from the far edge, so the near edge is 2750 - 38.5 - 10 = 2701.5, which rounds half up to 2702.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, RED), Some((2702, 0, 2712, 12)));
    assert_eq!(count_of(&frame, RED), 120);
}
