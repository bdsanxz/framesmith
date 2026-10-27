//! A place or a size that lies beyond the limit is not worked out (`docs/spec/render-command.md` section 4.1, rule 6).
//! An amount that is not a finite number is refused before it is placed, by the notation (N9), so layout never sees one.

use layout::{frame, place, Amount, Beyond, Written, LIMIT};

fn at(left: Option<Amount>, width: Option<Amount>) -> Result<layout::Rect, Beyond> {
    place(
        frame(96, 54),
        &Written {
            left,
            width,
            ..Written::default()
        },
    )
}

#[test]
fn an_edge_at_the_limit_is_placed_and_one_beyond_it_is_not() {
    let limit = LIMIT as f64;
    assert!(at(Some(Amount::Pixels((limit).into())), Some(Amount::Pixels((0.0).into()))).is_ok());
    assert!(at(Some(Amount::Pixels((-limit).into())), Some(Amount::Pixels((0.0).into()))).is_ok());
    assert_eq!(
        at(Some(Amount::Pixels((limit + 1.0).into())), Some(Amount::Pixels((0.0).into()))),
        Err(Beyond)
    );
    assert_eq!(
        at(Some(Amount::Pixels((-limit - 1.0).into())), Some(Amount::Pixels((0.0).into()))),
        Err(Beyond)
    );
}

#[test]
fn a_far_edge_beyond_the_limit_is_not_placed_though_its_near_edge_is() {
    let limit = LIMIT as f64;
    assert_eq!(
        at(Some(Amount::Pixels((limit).into())), Some(Amount::Pixels((1.0).into()))),
        Err(Beyond)
    );
}

#[test]
fn a_size_beyond_the_limit_is_not_placed() {
    let limit = LIMIT as f64;
    assert!(at(None, Some(Amount::Pixels((limit).into()))).is_ok());
    assert_eq!(at(None, Some(Amount::Pixels((limit + 1.0).into()))), Err(Beyond));
}

#[test]
fn a_percentage_too_large_for_the_limit_is_not_placed() {
    assert_eq!(at(Some(Amount::Percent((1e308).into())), None), Err(Beyond));
}

#[test]
fn an_amount_whose_digits_are_further_apart_than_the_work_bound_is_not_placed() {
    // A percentage of 1e-100001 has its digits 100000 places beyond the start edge's, which is more than the bound
    // allows to line up in one sum, so the place is refused rather than worked out.
    let tiny = layout::Decimal::from_parts(false, "1", -100_001).unwrap();
    assert_eq!(at(Some(Amount::Percent(tiny)), Some(Amount::Pixels((10.0).into()))), Err(Beyond));
}
