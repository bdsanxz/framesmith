//! The comparison of container facts, where no video is needed.

use parity::container;

#[test]
fn equal_rates_written_differently_are_the_same_rate() {
    assert!(container::same_rate("30/1", "30000/1000"));
    assert!(container::same_rate("30", "30/1"));
    assert!(!container::same_rate("30000/1001", "30/1"));
    assert!(!container::same_rate("30/1", "24/1"));
}
