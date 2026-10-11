use motion::*;
use timebase::Time;

fn timeline(json: &str, fps: u32) -> Timeline {
    let block = parse_block(json);
    assert_eq!(block.findings, vec![], "fixture must be valid");
    let tweens: Vec<Tween> = block.tweens.into_iter().map(|(tween, _)| tween).collect();
    let targets: Vec<Vec<usize>> = tweens.iter().map(|_| vec![0]).collect();
    Timeline::build(&tweens, &targets, fps)
}

fn at(timeline: &Timeline, prop: Prop, secs: &str) -> f64 {
    timeline.elements[0].tracks[prop.index()]
        .as_ref()
        .expect("the property is animated")
        .value(Time::parse(secs).unwrap())
}

#[test]
fn t016_c1_to_tween_moves_between_its_start_and_end() {
    let tl = timeline(r##"[{ "target": "#a", "to": { "x": 300 }, "at": 1, "duration": 1 }]"##, 30);
    assert_eq!(at(&tl, Prop::X, "0"), 0.0);
    assert_eq!(at(&tl, Prop::X, "1"), 0.0);
    assert_eq!(at(&tl, Prop::X, "1.5"), 150.0);
    assert_eq!(at(&tl, Prop::X, "2"), 300.0);
    assert_eq!(at(&tl, Prop::X, "5"), 300.0);
    assert!(tl.elements[0].tracks[Prop::Y.index()].is_none());
}

#[test]
fn t017_c1_from_tween_holds_its_start_before_it_begins() {
    let tl = timeline(r##"[{ "target": "#a", "from": { "y": 60 }, "at": 1, "duration": 0.5 }]"##, 30);
    assert_eq!(at(&tl, Prop::Y, "0"), 60.0);
    assert_eq!(at(&tl, Prop::Y, "0.9"), 60.0);
    assert_eq!(at(&tl, Prop::Y, "1.25"), 30.0);
    assert_eq!(at(&tl, Prop::Y, "1.5"), 0.0);
    assert_eq!(at(&tl, Prop::Y, "3"), 0.0);
}

#[test]
fn t017_c2_from_to_tween_holds_both_ends() {
    let tl = timeline(
        r##"[{ "target": "#a", "from": { "x": 100 }, "to": { "x": 200 }, "at": 1, "duration": 2 }]"##,
        30,
    );
    assert_eq!(at(&tl, Prop::X, "0.5"), 100.0);
    assert_eq!(at(&tl, Prop::X, "2"), 150.0);
    assert_eq!(at(&tl, Prop::X, "4"), 200.0);
}

#[test]
fn t017_c3_set_applies_from_its_frame_on() {
    let tl = timeline(r##"[{ "target": "#a", "set": { "x": 120 }, "at": 2 }]"##, 30);
    let frame = |i: i64| tl.elements[0].tracks[0].as_ref().unwrap().value(Time::of_frame(i, 30));
    assert_eq!(frame(59), 0.0);
    assert_eq!(frame(60), 120.0);
    assert_eq!(frame(61), 120.0);
}

#[test]
fn t017_c4_an_earlier_tween_holds_until_the_next_one_starts() {
    let tl = timeline(
        r##"[{ "target": "#a", "to": { "x": 100 }, "at": 0, "duration": 1 },
             { "target": "#a", "from": { "x": 400 }, "to": { "x": 500 }, "at": 3, "duration": 1 }]"##,
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
        r##"[{ "target": "#a", "to": { "x": 100 }, "at": 0, "duration": 2 },
             { "target": "#a", "to": { "x": 0 }, "at": 1, "duration": 1 }]"##,
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
        r##"[{ "target": "#a", "set": { "x": 40 }, "at": 0 },
             { "target": "#a", "from": { "x": 100 }, "at": 1, "duration": 1 }]"##,
        30,
    );
    assert_eq!(at(&tl, Prop::X, "0.5"), 40.0);
    assert_eq!(at(&tl, Prop::X, "1"), 100.0);
    assert_eq!(at(&tl, Prop::X, "2"), 40.0);
}

#[test]
fn tweens_with_the_same_start_keep_document_order() {
    let tl = timeline(
        r##"[{ "target": "#a", "set": { "x": 1 }, "at": 1 },
             { "target": "#a", "set": { "x": 2 }, "at": 1 }]"##,
        30,
    );
    assert_eq!(at(&tl, Prop::X, "1"), 2.0);
}

#[test]
fn t019_c2_eased_tweens_are_exact_at_both_ends() {
    for ease in Ease::ALL {
        let tl = timeline(
            &format!(
                r##"[{{ "target": "#a", "to": {{ "x": 300 }}, "at": 1, "duration": 1, "ease": "{}" }}]"##,
                ease.name()
            ),
            30,
        );
        assert_eq!(at(&tl, Prop::X, "1"), 0.0, "{}", ease.name());
        assert_eq!(at(&tl, Prop::X, "2"), 300.0, "{}", ease.name());
        assert_eq!(at(&tl, Prop::X, "1.5"), 300.0 * ease.apply(0.5), "{}", ease.name());
    }
}

#[test]
fn t017_c5_the_value_does_not_depend_on_evaluation_order() {
    let tl = timeline(
        r##"[{ "target": "#a", "from": { "x": -50, "opacity": 0 }, "at": 0.5, "duration": 1, "ease": "in-out-cubic" },
             { "target": "#a", "to": { "x": 80, "rotation": 90 }, "at": 2, "duration": 1, "ease": "out-quart" }]"##,
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
    let tl = timeline(r##"[{ "target": "#a", "set": { "x": 9 }, "at": 0.116667 }]"##, 60);
    let track = tl.elements[0].tracks[0].as_ref().unwrap();
    assert_eq!(track.value(Time::of_frame(6, 60)), 0.0);
    assert_eq!(track.value(Time::of_frame(7, 60)), 9.0);
}

#[test]
fn several_elements_get_the_same_motion() {
    let block = parse_block(r##"[{ "target": ".dot", "to": { "x": 30 }, "at": 0, "duration": 1 }]"##);
    let tweens: Vec<Tween> = block.tweens.into_iter().map(|(tween, _)| tween).collect();
    let tl = Timeline::build(&tweens, &[vec![4, 7, 9]], 30);
    assert_eq!(tl.elements.iter().map(|e| e.element).collect::<Vec<_>>(), vec![4, 7, 9]);
    for element in &tl.elements {
        assert_eq!(element.values(Time::parse("0.5").unwrap())[0], Some(15.0));
    }
}

#[test]
fn motion_end_is_the_last_tween_end() {
    let block = parse_block(
        r##"[{ "target": "#a", "to": { "x": 1 }, "at": 4, "duration": 1 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 2 }]"##,
    );
    let tweens: Vec<Tween> = block.tweens.into_iter().map(|(tween, _)| tween).collect();
    assert_eq!(motion_end(&tweens), Some(Time::from_secs(5)));
    assert_eq!(motion_end(&[]), None);
}
