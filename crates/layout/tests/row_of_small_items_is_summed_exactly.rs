//! The edges of a start-aligned row are sums of written amounts, so they are exact before they are rounded
//! (`docs/spec/render-command.md`, section 4.3). A float running sum drifts by a fraction of a pixel in a long row.

use layout::{flow, frame, Amount, Flex, Item, Written};

#[test]
fn the_thirtieth_of_thirty_items_of_0_point_35_pixels_ends_at_10_point_5_and_is_placed_to_11() {
    // Each item is 0.35 pixels, so the thirtieth starts at 29 × 0.35 = 10.15 and ends at 30 × 0.35 = 10.5. Its left edge
    // rounds to 10 and its far edge, a half up, to 11: the item is one pixel wide. A running sum of floats reaches
    // 10.499999999999998 and would end it at 10, a width of 0.
    let items: Vec<Item> = (0..30)
        .map(|_| Item {
            written: Written {
                width: Some(Amount::Pixels((0.35).into())),
                ..Written::default()
            },
            ..Item::default()
        })
        .collect();
    let flowed = flow(frame(400, 300), &Written::default(), &Flex::default(), &items).expect("placed");
    let last = flowed.items[29];
    assert_eq!((last.left, last.width), (10, 1));
}
