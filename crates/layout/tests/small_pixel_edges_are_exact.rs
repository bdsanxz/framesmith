//! A pixel amount is kept as it is written, so a fraction of a pixel is not lost before an edge is rounded
//! (`docs/spec/render-command.md`, section 4.1, rule 3). The values are worked by hand from that rule.

use layout::{frame, place, Amount, Decimal, Rect, Written};

fn across(parent: Rect, left: Option<Amount>, width: Option<Amount>) -> (i32, u32) {
    let rect = place(
        parent,
        &Written {
            left,
            width,
            ..Written::default()
        },
    )
    .unwrap();
    (rect.left, rect.width)
}

#[test]
fn two_fractions_of_a_pixel_round_each_edge_once_and_the_far_edge_is_0_point_5() {
    // The left edge is 0.35 and the far edge 0.35 + 0.15 = 0.5, which a half up rounds to 1. The left edge rounds
    // to 0, so the box is one pixel wide. Rounding its size first would leave it 0 wide.
    let parent = frame(96, 54);
    assert_eq!(
        across(parent, Some(Amount::Pixels((0.35).into())), Some(Amount::Pixels((0.15).into())),),
        (0, 1)
    );
}

#[test]
fn a_written_exponent_is_read_as_written() {
    // `3.85e1` is 38.5: the digits 385 at exponent -1. A half up rounds it to 39, as `38.5` does.
    let parent = frame(96, 54);
    let written = Decimal::from_parts(false, "385", -1).unwrap();
    assert_eq!(
        across(parent, Some(Amount::Pixels(written)), Some(Amount::Pixels((10.0).into()))),
        (39, 10)
    );
}
