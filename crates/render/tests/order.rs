//! The order frames are captured in (a maintainer switch).

use render::Order;

#[test]
fn the_written_forms_are_read() {
    assert_eq!(Order::parse("reverse"), Order::Reverse);
    assert_eq!(Order::parse("59, 25,6"), Order::First(vec![59, 25, 6]));
    assert_eq!(Order::parse("backwards"), Order::Forward);
    assert_eq!(Order::parse("3,x"), Order::Forward);
}

#[test]
fn every_frame_is_captured_once_in_each_order() {
    assert_eq!(Order::Forward.of(4), [0, 1, 2, 3]);
    assert_eq!(Order::Reverse.of(4), [3, 2, 1, 0]);
    assert_eq!(Order::First(vec![5, 2]).of(6), [5, 2, 0, 1, 3, 4]);
    // A frame past the end or written twice is ignored.
    assert_eq!(Order::First(vec![9, 3, 3, 0]).of(4), [3, 0, 1, 2]);
    assert_eq!(Order::Reverse.of(0), [0u64; 0]);
}
