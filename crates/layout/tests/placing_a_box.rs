//! Where a box that stands directly in the frame lies, and which of its pixels are seen
//! (`docs/spec/render-command.md` section 4, `docs/spec/notation.md` section 7.1).

use layout::{place, Rect, Visible, Written};

#[test]
fn a_box_with_nothing_written_is_as_large_as_the_frame_at_its_corner() {
    assert_eq!(
        place(640, 360, Written::default()),
        Rect {
            left: 0,
            top: 0,
            width: 640,
            height: 360
        }
    );
}

#[test]
fn what_is_written_is_taken_and_what_is_not_is_the_default() {
    let rect = place(
        640,
        360,
        Written {
            left: Some(-5),
            top: Some(12),
            width: None,
            height: Some(0),
        },
    );
    assert_eq!(
        rect,
        Rect {
            left: -5,
            top: 12,
            width: 640,
            height: 0
        }
    );
}

#[test]
fn a_box_inside_the_frame_is_seen_whole() {
    let rect = Rect {
        left: 12,
        top: 8,
        width: 40,
        height: 30,
    };
    assert_eq!(
        rect.visible(96, 54),
        Some(Visible {
            left: 12,
            top: 8,
            right: 52,
            bottom: 38
        })
    );
}

#[test]
fn the_part_of_a_box_outside_the_frame_is_cut_off_on_every_side() {
    let big = Rect {
        left: -10,
        top: -20,
        width: 1000,
        height: 1000,
    };
    assert_eq!(
        big.visible(96, 54),
        Some(Visible {
            left: 0,
            top: 0,
            right: 96,
            bottom: 54
        })
    );
    let corner = Rect {
        left: 90,
        top: 50,
        width: 20,
        height: 20,
    };
    assert_eq!(
        corner.visible(96, 54),
        Some(Visible {
            left: 90,
            top: 50,
            right: 96,
            bottom: 54
        })
    );
    let before = Rect {
        left: -30,
        top: 10,
        width: 40,
        height: 5,
    };
    assert_eq!(
        before.visible(96, 54),
        Some(Visible {
            left: 0,
            top: 10,
            right: 10,
            bottom: 15
        })
    );
}

#[test]
fn a_box_with_no_pixel_in_the_frame_is_not_seen() {
    for (left, top, width, height) in [
        (96, 0, 10, 10),
        (0, 54, 10, 10),
        (-10, 0, 10, 10),
        (0, -10, 10, 10),
        (5, 5, 0, 10),
        (5, 5, 10, 0),
        (i32::MIN, 0, 10, 10),
        (i32::MAX, 0, u32::MAX, 10),
    ] {
        let rect = Rect { left, top, width, height };
        assert_eq!(rect.visible(96, 54), None, "{rect:?}");
    }
}

#[test]
fn a_large_box_does_not_overflow() {
    let rect = Rect {
        left: i32::MAX,
        top: i32::MAX,
        width: u32::MAX,
        height: u32::MAX,
    };
    assert_eq!(rect.visible(8192, 8192), None);
    let rect = Rect {
        left: i32::MIN,
        top: i32::MIN,
        width: u32::MAX,
        height: u32::MAX,
    };
    assert_eq!(
        rect.visible(8192, 8192),
        Some(Visible {
            left: 0,
            top: 0,
            right: 8192,
            bottom: 8192
        })
    );
}
