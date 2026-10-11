use timebase::*;

fn t(text: &str) -> Time {
    Time::parse(text).unwrap()
}

#[test]
fn reads_decimals_exactly() {
    assert_eq!(t("0.1"), Time::new(1, 10));
    assert_eq!(t("6.01"), Time::new(601, 100));
    assert_eq!(t("-2"), Time::from_secs(-2));
    assert_eq!(t("0.1") + t("0.2"), t("0.3"));
    for bad in ["", ".", "1.", "1e3", "abc", "1.0000000001", "+1", "1 s", "--1", "0x10"] {
        assert_eq!(Time::parse(bad), None, "{bad}");
    }
    assert_eq!(Time::parse(".5"), Some(Time::new(1, 2)));
}

#[test]
fn t005_c1_six_seconds_at_24_30_60() {
    assert_eq!(frame_count(t("6"), 24), 144);
    assert_eq!(frame_count(t("6"), 30), 180);
    assert_eq!(frame_count(t("6"), 60), 360);
}

#[test]
fn t005_c2_six_point_zero_one_rounds_up() {
    assert_eq!(frame_count(t("6.01"), 30), 181);
}

#[test]
fn t005_c3_within_a_thousandth_takes_the_whole_number() {
    // 6.000016667 s × 30 = 180.0005 frames.
    assert_eq!(frame_count(t("6.000016667"), 30), 180);
    // 180.001 is still inside the tolerance; 180.0011 is not.
    assert_eq!(frame_count(Time::new(180_001, 30_000), 30), 180);
    assert_eq!(frame_count(Time::new(1_800_011, 300_000), 30), 181);
    // Just below a whole number rounds up to it either way.
    assert_eq!(frame_count(t("5.99999"), 30), 180);
    assert_eq!(frame_count(t("0.00001"), 30), 1);
}

#[test]
fn frame_time_is_exact() {
    assert_eq!(Time::of_frame(7, 60), Time::new(7, 60));
    assert_eq!(Time::of_frame(90, 30), Time::from_secs(3));
    assert!(Time::of_frame(29, 30) < Time::from_secs(1));
}

#[test]
fn t011_c3_seven_sixtieths_lands_on_frame_seven() {
    // 7/60 s cannot be written exactly; these are what an author writes.
    for written in ["0.116667", "0.1166667", "0.11666667", "0.116666667"] {
        assert_eq!(t(written).grid_frame(60), 7, "{written}");
    }
    // One tenth of a second is frame 6 at 60, although 0.1 × 60 is not 6 in binary.
    assert_eq!(t("0.1").grid_frame(60), 6);
    assert_ne!((0.1f64 * 3.0 * 20.0).to_bits(), 6.0f64.to_bits());
}

#[test]
fn grid_rounds_up_outside_the_tolerance() {
    assert_eq!(t("1.01").grid_frame(30), 31);
    assert_eq!(t("1").grid_frame(30), 30);
    assert_eq!(t("0.99").grid_frame(30), 30);
}

#[test]
fn snaps_only_near_a_frame_time() {
    assert_eq!(t("0.116667").snapped(60), Time::new(7, 60));
    assert_eq!(t("0.1166").snapped(60), t("0.1166"));
    assert_eq!(t("0.999999").snapped(30), Time::from_secs(1));
}

#[test]
fn shows_times_as_decimals() {
    assert_eq!(t("6").to_string(), "6");
    assert_eq!(t("6.010").to_string(), "6.01");
    assert_eq!(t("0.5").to_string(), "0.5");
    assert_eq!(Time::new(1, 3).to_string(), "0.333333");
    assert_eq!(t("-0.25").to_string(), "-0.25");
}

#[test]
fn exact_form_round_trips() {
    for time in [t("6.01"), Time::new(7, 60), Time::ZERO, t("-3.5")] {
        assert_eq!(Time::from_exact(&time.to_exact()), Some(time));
    }
    assert_eq!(Time::from_exact("1/0"), None);
    assert_eq!(Time::from_exact("x"), None);
}

#[test]
fn reads_frame_rates() {
    assert_eq!(parse_fps("30"), Ok(30));
    assert_eq!(parse_fps("1"), Ok(1));
    assert_eq!(parse_fps("240"), Ok(240));
    assert_eq!(parse_fps("30000/1001"), Err(FpsError::Ratio));
    for bad in ["0", "241", "-30", "29.97", "fast", "", "30fps", "1/x", "0030"] {
        assert_eq!(parse_fps(bad), Err(FpsError::Invalid), "{bad}");
    }
}
