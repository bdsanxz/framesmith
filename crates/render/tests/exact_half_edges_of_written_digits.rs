//! The edges of boxes whose amounts are written with an exponent, with more digits than a float holds, and as items of a row
//! that meet on an exact half, are the whole pixels `docs/spec/render-command.md` section 4.1, rule 3 and section 4.3, rule 9
//! give, on the processor. The frame equals its checked-in picture to the byte. The columns are worked by hand in the
//! comment of the fixture (`fixtures/box-edges-exact-half/composition.xml`).

mod common;

use common::{compare, count_of, expected, extent_of, numbers_with_a_picture, project_from_fixture};
use draw::Picture;
use draw_cpu::CpuRenderer;
use project::Project;
use render::Scene;

const FIXTURE: &str = "box-edges-exact-half";
const LEAD: [u8; 4] = [0xe0, 0xa0, 0x30, 0xff];
const EXPONENT: [u8; 4] = [0x30, 0xc0, 0xa0, 0xff];
const JUST_BELOW: [u8; 4] = [0xa0, 0x50, 0xd0, 0xff];
const FIRST_ITEM: [u8; 4] = [0xd0, 0x50, 0x30, 0xff];
const SECOND_ITEM: [u8; 4] = [0xf0, 0xd0, 0x40, 0xff];
const BACKGROUND: [u8; 4] = [0x10, 0x20, 0x40, 0xff];

fn drawn_frame(number: u64) -> Picture {
    let folder = project_from_fixture("exact-half-edges-of-written-digits", FIXTURE, FIXTURE);
    let scene =
        Scene::from_project(&Project::open(&folder).unwrap(), &render::system_codecs()).unwrap_or_else(|reports| panic!("{reports:?}"));
    scene.picture(number, &mut CpuRenderer).unwrap()
}

#[test]
fn the_frame_equals_its_checked_in_picture() {
    assert_eq!(numbers_with_a_picture(FIXTURE), vec![0], "the first frame has a picture");
    if let Err(difference) = compare(0, &drawn_frame(0), &expected(FIXTURE, 0)) {
        panic!("{FIXTURE}: {difference}");
    }
}

#[test]
fn a_box_written_with_an_exponent_covers_the_same_columns_as_its_decimal() {
    // 14e-1% is 1.4% of 2750, exactly 38.5: the edges 38.5 and 48.5 round half up to 39 and 49, so columns 39 to 48.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, LEAD), Some((39, 0, 49, 2)));
    assert_eq!(count_of(&frame, LEAD), 20);
    assert_eq!(extent_of(&frame, EXPONENT), Some((39, 2, 49, 4)));
    assert_eq!(count_of(&frame, EXPONENT), 20);
}

#[test]
fn a_percentage_just_below_a_half_with_21_digits_keeps_its_digits_and_rounds_down() {
    // 1.39999999999999999999% of 2750 is 38.499999999999999999725: the near edge rounds down to 38 and the far edge,
    // 48.499999999999999999725, rounds down to 48, so columns 38 to 47. A reader that kept only 1.4 would place the
    // box at 39.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, JUST_BELOW), Some((38, 4, 48, 6)));
    assert_eq!(count_of(&frame, JUST_BELOW), 20);
    assert_eq!(frame.pixel(37, 4), Some(BACKGROUND));
    assert_eq!(frame.pixel(38, 4), Some(JUST_BELOW));
    assert_eq!(frame.pixel(47, 4), Some(JUST_BELOW));
    assert_eq!(frame.pixel(48, 4), Some(BACKGROUND));
}

#[test]
fn two_items_of_a_row_that_meet_on_an_exact_half_share_one_pixel_edge() {
    // Each item is 1.4% of 2750, exactly 38.5. The first runs from 0 to 38.5, columns 0 to 38. The second runs from 38.5
    // to 77, and its near edge rounds to the same 39 as the first item's far edge, so it covers columns 39 to 76.
    let frame = drawn_frame(0);
    assert_eq!(extent_of(&frame, FIRST_ITEM), Some((0, 6, 39, 8)));
    assert_eq!(count_of(&frame, FIRST_ITEM), 78);
    assert_eq!(extent_of(&frame, SECOND_ITEM), Some((39, 6, 77, 8)));
    assert_eq!(count_of(&frame, SECOND_ITEM), 76);
    assert_eq!(frame.pixel(38, 6), Some(FIRST_ITEM));
    assert_eq!(frame.pixel(39, 6), Some(SECOND_ITEM));
    assert_eq!(frame.pixel(76, 6), Some(SECOND_ITEM));
    assert_eq!(frame.pixel(77, 6), Some(BACKGROUND));
}
