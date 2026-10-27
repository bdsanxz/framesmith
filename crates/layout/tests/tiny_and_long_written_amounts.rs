//! A written amount keeps every digit it is written with, and a very small one is a share of the parent, not an
//! error (`docs/spec/render-command.md`, section 4.1, rules 3 and 6). The values are worked by hand.

use layout::{frame, place, Amount, Decimal, Rect, Written};

fn left_and_width(parent: Rect, left: Option<Amount>, width: Option<Amount>) -> Result<(i32, u32), layout::Beyond> {
    let rect = place(
        parent,
        &Written {
            left,
            width,
            ..Written::default()
        },
    )?;
    Ok((rect.left, rect.width))
}

#[test]
fn a_long_written_amount_keeps_every_digit_so_it_rounds_down() {
    // 38.4999999999999999 has 18 significant digits. Its nearest float is 38.5, which a half up rounds to 39; the
    // amount as written lies below the half, so it rounds to 38.
    let written = Decimal::from_parts(false, "384999999999999999", -16).unwrap();
    assert_eq!(
        left_and_width(frame(96, 54), Some(Amount::Pixels(written)), Some(Amount::Pixels((10.0).into()))),
        Ok((38, 10))
    );
}

#[test]
fn a_percentage_of_1e_minus_40_is_a_share_of_the_parent_that_places_at_0() {
    // 1e-40% of 96 is 9.6e-39 pixels, far below a half, so the edge is 0 and the box keeps its width.
    let tiny = Decimal::from_parts(false, "1", -40).unwrap();
    assert_eq!(
        left_and_width(frame(96, 54), Some(Amount::Percent(tiny)), Some(Amount::Pixels((10.0).into()))),
        Ok((0, 10))
    );
}

#[test]
fn a_percentage_of_1e_minus_300_is_a_share_of_the_parent_that_places_at_0() {
    // The same, one step further down the exponent: the arithmetic keeps the amount and its edge is 0.
    let tiny = Decimal::from_parts(false, "1", -300).unwrap();
    assert_eq!(
        left_and_width(frame(96, 54), Some(Amount::Percent(tiny)), Some(Amount::Pixels((10.0).into()))),
        Ok((0, 10))
    );
}
