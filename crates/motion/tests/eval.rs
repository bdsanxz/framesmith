use motion::*;
use timebase::Time;

const NO_VALUES: Values = [None; 5];

fn values(list: &[(Prop, f64)]) -> Values {
    let mut values = NO_VALUES;
    for &(prop, value) in list {
        values[prop.index()] = Some(value);
    }
    values
}

fn secs(text: &str) -> Time {
    Time::parse(text).unwrap()
}

fn to(list: &[(Prop, f64)], at: &str, duration: &str, ease: Ease) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::To,
        from: NO_VALUES,
        to: values(list),
        at: secs(at),
        duration: secs(duration),
        ease,
    }
}

fn from(list: &[(Prop, f64)], at: &str, duration: &str, ease: Ease) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::From,
        from: values(list),
        to: NO_VALUES,
        at: secs(at),
        duration: secs(duration),
        ease,
    }
}

fn from_to(start: &[(Prop, f64)], end: &[(Prop, f64)], at: &str, duration: &str, ease: Ease) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::FromTo,
        from: values(start),
        to: values(end),
        at: secs(at),
        duration: secs(duration),
        ease,
    }
}

fn set(list: &[(Prop, f64)], at: &str) -> Tween {
    Tween {
        target: "#a".into(),
        kind: Kind::Set,
        from: NO_VALUES,
        to: values(list),
        at: secs(at),
        duration: Time::ZERO,
        ease: Ease::Linear,
    }
}

fn timeline(tweens: &[Tween], fps: u32) -> Timeline {
    let targets: Vec<Vec<usize>> = tweens.iter().map(|_| vec![0]).collect();
    Timeline::build(tweens, &targets, fps)
}

fn at(timeline: &Timeline, prop: Prop, secs: &str) -> f64 {
    timeline.elements[0].tracks[prop.index()]
        .as_ref()
        .expect("the property is animated")
        .value(Time::parse(secs).unwrap())
}

#[test]
fn to_tween_moves_between_its_start_and_end() {
    let tl = timeline(&[to(&[(Prop::X, 300.0)], "1", "1", Ease::Linear)], 30);
    assert_eq!(at(&tl, Prop::X, "0"), 0.0);
    assert_eq!(at(&tl, Prop::X, "1"), 0.0);
    assert_eq!(at(&tl, Prop::X, "1.5"), 150.0);
    assert_eq!(at(&tl, Prop::X, "2"), 300.0);
    assert_eq!(at(&tl, Prop::X, "5"), 300.0);
    assert!(tl.elements[0].tracks[Prop::Y.index()].is_none());
}

#[test]
fn from_tween_holds_its_start_before_it_begins() {
    let tl = timeline(&[from(&[(Prop::Y, 60.0)], "1", "0.5", Ease::Linear)], 30);
    assert_eq!(at(&tl, Prop::Y, "0"), 60.0);
    assert_eq!(at(&tl, Prop::Y, "0.9"), 60.0);
    assert_eq!(at(&tl, Prop::Y, "1.25"), 30.0);
    assert_eq!(at(&tl, Prop::Y, "1.5"), 0.0);
    assert_eq!(at(&tl, Prop::Y, "3"), 0.0);
}

#[test]
fn from_to_tween_holds_both_ends() {
    let tl = timeline(&[from_to(&[(Prop::X, 100.0)], &[(Prop::X, 200.0)], "1", "2", Ease::Linear)], 30);
    assert_eq!(at(&tl, Prop::X, "0.5"), 100.0);
    assert_eq!(at(&tl, Prop::X, "2"), 150.0);
    assert_eq!(at(&tl, Prop::X, "4"), 200.0);
}

#[test]
fn set_applies_from_its_frame_on() {
    let tl = timeline(&[set(&[(Prop::X, 120.0)], "2")], 30);
    let frame = |i: i64| tl.elements[0].tracks[0].as_ref().unwrap().value(Time::of_frame(i, 30));
    assert_eq!(frame(59), 0.0);
    assert_eq!(frame(60), 120.0);
    assert_eq!(frame(61), 120.0);
}

#[test]
fn an_earlier_tween_holds_until_the_next_one_starts() {
    let tl = timeline(
        &[
            to(&[(Prop::X, 100.0)], "0", "1", Ease::Linear),
            from_to(&[(Prop::X, 400.0)], &[(Prop::X, 500.0)], "3", "1", Ease::Linear),
        ],
        30,
    );
    assert_eq!(at(&tl, Prop::X, "1"), 100.0);
    assert_eq!(at(&tl, Prop::X, "2.9"), 100.0);
    assert_eq!(at(&tl, Prop::X, "3"), 400.0);
    assert_eq!(at(&tl, Prop::X, "4"), 500.0);
}

#[test]
fn a_to_tween_starts_from_the_value_in_effect() {
    let tl = timeline(
        &[
            to(&[(Prop::X, 100.0)], "0", "2", Ease::Linear),
            to(&[(Prop::X, 0.0)], "1", "1", Ease::Linear),
        ],
        30,
    );
    // The second tween interrupts the first half-way, at x = 50.
    assert_eq!(at(&tl, Prop::X, "1"), 50.0);
    assert_eq!(at(&tl, Prop::X, "1.5"), 25.0);
    assert_eq!(at(&tl, Prop::X, "2"), 0.0);
}

#[test]
fn a_from_tween_ends_at_the_value_in_effect() {
    let tl = timeline(
        &[set(&[(Prop::X, 40.0)], "0"), from(&[(Prop::X, 100.0)], "1", "1", Ease::Linear)],
        30,
    );
    assert_eq!(at(&tl, Prop::X, "0.5"), 40.0);
    assert_eq!(at(&tl, Prop::X, "1"), 100.0);
    assert_eq!(at(&tl, Prop::X, "2"), 40.0);
}

#[test]
fn tweens_with_the_same_start_keep_document_order() {
    let tl = timeline(&[set(&[(Prop::X, 1.0)], "1"), set(&[(Prop::X, 2.0)], "1")], 30);
    assert_eq!(at(&tl, Prop::X, "1"), 2.0);
}

#[test]
fn eased_tweens_are_exact_at_both_ends() {
    for ease in Ease::ALL {
        let tl = timeline(&[to(&[(Prop::X, 300.0)], "1", "1", ease)], 30);
        assert_eq!(at(&tl, Prop::X, "1"), 0.0, "{}", ease.name());
        assert_eq!(at(&tl, Prop::X, "2"), 300.0, "{}", ease.name());
        assert_eq!(at(&tl, Prop::X, "1.5"), 300.0 * ease.apply(0.5), "{}", ease.name());
    }
}

#[test]
fn the_value_does_not_depend_on_evaluation_order() {
    let tl = timeline(
        &[
            from(&[(Prop::X, -50.0), (Prop::Opacity, 0.0)], "0.5", "1", Ease::InOutCubic),
            to(&[(Prop::X, 80.0), (Prop::Rotation, 90.0)], "2", "1", Ease::OutQuart),
        ],
        30,
    );
    let forward: Vec<[Option<f64>; 5]> = (0..120).map(|i| tl.elements[0].values(Time::of_frame(i, 30))).collect();
    let mut backward: Vec<[Option<f64>; 5]> = (0..120).rev().map(|i| tl.elements[0].values(Time::of_frame(i, 30))).collect();
    backward.reverse();
    assert_eq!(forward, backward);
    let bits = |values: &[Option<f64>; 5]| values.map(|v| v.map(f64::to_bits));
    for (a, b) in forward.iter().zip(&backward) {
        assert_eq!(bits(a), bits(b));
    }
}

#[test]
fn times_near_a_frame_are_taken_as_that_frame() {
    // 0.116667 s is meant as frame 7 at 60 frames per second.
    let tl = timeline(&[set(&[(Prop::X, 9.0)], "0.116667")], 60);
    let track = tl.elements[0].tracks[0].as_ref().unwrap();
    assert_eq!(track.value(Time::of_frame(6, 60)), 0.0);
    assert_eq!(track.value(Time::of_frame(7, 60)), 9.0);
}

#[test]
fn several_elements_get_the_same_motion() {
    let tweens = [to(&[(Prop::X, 30.0)], "0", "1", Ease::Linear)];
    let tl = Timeline::build(&tweens, &[vec![4, 7, 9]], 30);
    assert_eq!(tl.elements.iter().map(|e| e.element).collect::<Vec<_>>(), vec![4, 7, 9]);
    for element in &tl.elements {
        assert_eq!(element.values(Time::parse("0.5").unwrap())[0], Some(15.0));
    }
}

#[test]
fn motion_end_is_the_last_tween_end() {
    let tweens = [
        to(&[(Prop::X, 1.0)], "4", "1", Ease::Linear),
        to(&[(Prop::X, 1.0)], "0", "2", Ease::Linear),
    ];
    assert_eq!(motion_end(&tweens), Some(Time::from_secs(5)));
    assert_eq!(motion_end(&[]), None);
}
