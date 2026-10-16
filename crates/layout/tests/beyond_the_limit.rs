//! A place or a size that lies beyond the limit, or an amount that is not finite, is not worked out
//! (`docs/spec/render-command.md` section 4.1, rule 6).

use layout::{frame, place, Amount, Beyond, Written, LIMIT};

fn at(left: Option<Amount>, width: Option<Amount>) -> Result<layout::Rect, Beyond> {
    place(
        frame(96, 54),
        Written {
            left,
            width,
            ..Written::default()
        },
    )
}

#[test]
fn an_edge_at_the_limit_is_placed_and_one_beyond_it_is_not() {
    let limit = LIMIT as f64;
    assert!(at(Some(Amount::Pixels(limit)), Some(Amount::Pixels(0.0))).is_ok());
    assert!(at(Some(Amount::Pixels(-limit)), Some(Amount::Pixels(0.0))).is_ok());
    assert_eq!(at(Some(Amount::Pixels(limit + 1.0)), Some(Amount::Pixels(0.0))), Err(Beyond));
    assert_eq!(at(Some(Amount::Pixels(-limit - 1.0)), Some(Amount::Pixels(0.0))), Err(Beyond));
}

#[test]
fn a_far_edge_beyond_the_limit_is_not_placed_though_its_near_edge_is() {
    let limit = LIMIT as f64;
    assert_eq!(at(Some(Amount::Pixels(limit)), Some(Amount::Pixels(1.0))), Err(Beyond));
}

#[test]
fn a_size_beyond_the_limit_is_not_placed() {
    let limit = LIMIT as f64;
    assert!(at(None, Some(Amount::Pixels(limit))).is_ok());
    assert_eq!(at(None, Some(Amount::Pixels(limit + 1.0))), Err(Beyond));
}

#[test]
fn an_amount_that_is_not_a_finite_number_is_not_placed() {
    assert_eq!(at(Some(Amount::Pixels(f64::INFINITY)), None), Err(Beyond));
    assert_eq!(at(Some(Amount::Pixels(f64::NAN)), None), Err(Beyond));
    assert_eq!(at(Some(Amount::Percent(1e308)), None), Err(Beyond));
}
