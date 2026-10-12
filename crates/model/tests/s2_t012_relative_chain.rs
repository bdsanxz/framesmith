//! Sprint 2, ticket T012, at the level of the resolver: a clip may follow a clip whose own start is
//! relative, to any depth, in whatever order the clips are written; starts that depend on each other in a
//! circle are one error (FS-E053) that names every clip of the circle; and a clip that follows a clip
//! whose start is open gets no error of its own. The page and the host resolve with this code, so these
//! rules hold in both.

use model::{Draft, RawAttr, RawDoc, RawElement, Resolved};
use timebase::Time;

fn element(tag: &str, id: Option<&str>, parent: Option<usize>, line: u32, attrs: &[(&str, &str)]) -> RawElement {
    RawElement {
        tag: tag.to_string(),
        id: id.map(str::to_string),
        attrs: attrs
            .iter()
            .map(|(name, value)| RawAttr {
                name: name.to_string(),
                value: value.to_string(),
                line,
            })
            .collect(),
        parent,
        line,
    }
}

fn composition(length: &str) -> RawElement {
    element(
        "div",
        None,
        None,
        3,
        &[
            ("data-fs-composition", "main"),
            ("data-fs-size", "64x36"),
            ("data-fs-duration", length),
        ],
    )
}

fn clip(id: &str, start: &str, duration: Option<&str>, line: u32) -> RawElement {
    let mut attrs = vec![("data-fs-clip", ""), ("data-fs-start", start)];
    if let Some(duration) = duration {
        attrs.push(("data-fs-duration", duration));
    }
    element("div", Some(id), Some(0), line, &attrs)
}

fn resolve_in(length: &str, clips: Vec<RawElement>) -> Resolved {
    let mut elements = vec![composition(length)];
    elements.extend(clips);
    let raw = RawDoc { elements, motion: vec![] };
    Draft::read(&raw, "index.html").finish(&mut |_| Ok(vec![]))
}

fn resolve(clips: Vec<RawElement>) -> Resolved {
    resolve_in("20", clips)
}

fn codes(resolved: &Resolved) -> Vec<&str> {
    resolved.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn time(text: &str) -> Time {
    Time::parse(text).unwrap()
}

/// The starts of the clips of the manifest, in document order.
fn starts(resolved: &Resolved) -> Vec<Time> {
    resolved
        .manifest
        .as_ref()
        .expect("a manifest")
        .clips
        .iter()
        .map(|clip| clip.start)
        .collect()
}

/// A line number for the clip written at position `at` (they are written one to a line from line 4).
fn line(at: usize) -> u32 {
    4 + at as u32
}

/// `#a` from 1 s for `a_duration`, `#b` after `#a`, `#c` after `#b` with an offset on each link.
fn chain(a_duration: &str) -> Resolved {
    resolve(vec![
        clip("a", "1", Some(a_duration), 4),
        clip("b", "after:a+0.25", Some("1.5"), 5),
        clip("c", "after:b-0.5", Some("1"), 6),
    ])
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut all = Vec::new();
    for at in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(at);
        for mut tail in permutations(&rest) {
            tail.insert(0, head);
            all.push(tail);
        }
    }
    all
}

// ---------------------------------------------------------------------
// c1: the starts of a chain, and what moves when the first duration changes
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c1_each_clip_of_a_chain_starts_at_the_end_of_the_clip_before_it_moved_by_its_own_offset() {
    // #a 1 s for 2 s ends at 3 s; #b is 3.25 s for 1.5 s and ends at 4.75 s; #c is 4.25 s.
    let resolved = chain("2");
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(starts(&resolved), [time("1"), time("3.25"), time("4.25")]);
    let clips = resolved.manifest.unwrap().clips;
    assert_eq!(clips[1].relative_start.as_deref(), Some("after:a+0.25"));
    assert_eq!(clips[2].relative_start.as_deref(), Some("after:b-0.5"));
    // The clips that are named are not changed.
    assert_eq!((clips[0].start, clips[0].duration), (time("1"), time("2")));
    assert_eq!((clips[1].duration, clips[2].duration), (time("1.5"), time("1")));
}

#[test]
fn s2_t012_c1_changing_the_duration_of_the_first_clip_by_one_second_moves_the_next_two_by_exactly_one_second() {
    let before = starts(&chain("2"));
    let after = starts(&chain("3"));
    assert_eq!(after[0], before[0]);
    assert_eq!(after[1], before[1] + time("1"));
    assert_eq!(after[2], before[2] + time("1"));
    // A shorter first clip moves them back by the same amount, to a start that may be before the first.
    let shorter = starts(&chain("1.5"));
    assert_eq!(shorter[1], before[1] - time("0.5"));
    assert_eq!(shorter[2], before[2] - time("0.5"));
    // Changing the start of the first clip moves them as well.
    let moved = resolve(vec![
        clip("a", "2", Some("2"), 4),
        clip("b", "after:a+0.25", Some("1.5"), 5),
        clip("c", "after:b-0.5", Some("1"), 6),
    ]);
    assert_eq!(starts(&moved), [time("2"), time("4.25"), time("5.25")]);
}

#[test]
fn s2_t012_c1_the_sum_over_several_links_is_exact_and_nothing_is_rounded_on_the_way() {
    // 0.1 + 0.2 over two links is exactly 0.3, and one third of a frame at ten frames a second
    // (0.033333333 s, nine decimals) is added as written.
    let resolved = resolve(vec![
        clip("a", "0", Some("0.1"), 4),
        clip("b", "after:a+0.1", Some("0.1"), 5),
        clip("c", "after:b+0.1", Some("0.1"), 6),
    ]);
    assert_eq!(starts(&resolved), [time("0"), time("0.2"), time("0.4")]);
    let resolved = resolve(vec![
        clip("a", "0", Some("0.1"), 4),
        clip("b", "after:a+0.2", Some("0.1"), 5),
        clip("c", "after:b-0.1", Some("0.1"), 6),
    ]);
    assert_eq!(starts(&resolved)[1], time("0.3"));
    assert_eq!(starts(&resolved)[2], time("0.3"));
    let resolved = resolve(vec![
        clip("a", "0", Some("0.1"), 4),
        clip("b", "after:a+0.033333333", Some("0.1"), 5),
        clip("c", "after:b+0.033333333", Some("0.1"), 6),
    ]);
    assert_eq!(starts(&resolved)[2], time("0.266666666"));
}

#[test]
fn s2_t012_c1_a_link_that_starts_before_zero_is_fs_e052_for_that_link_and_the_clips_after_it_say_nothing() {
    let resolved = resolve(vec![
        clip("a", "1", Some("1"), 4),
        clip("b", "after:a-5", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
        clip("d", "after:c+1", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E052"]);
    assert_eq!(resolved.diagnostics[0].line, 5);
    assert!(
        resolved.diagnostics[0].description.contains("#b starts at -3 s"),
        "{}",
        resolved.diagnostics[0].description
    );
    assert!(resolved.manifest.is_none());
    // A chain that reaches exactly zero is accepted.
    let resolved = resolve(vec![
        clip("a", "1", Some("1"), 4),
        clip("b", "after:a-2", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
    ]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(starts(&resolved), [time("1"), time("0"), time("1")]);
}

// ---------------------------------------------------------------------
// c2: whatever order the clips are written in
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c2_all_six_orders_of_a_chain_of_three_give_the_same_starts() {
    let written: Vec<(&str, &str, &str)> = vec![("a", "1", "2"), ("b", "after:a", "1"), ("c", "after:b+0.5", "1")];
    for order in permutations(&[0, 1, 2]) {
        let clips: Vec<RawElement> = order
            .iter()
            .enumerate()
            .map(|(place, &which)| clip(written[which].0, written[which].1, Some(written[which].2), line(place)))
            .collect();
        let resolved = resolve(clips);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{order:?}");
        let manifest = resolved.manifest.unwrap();
        for (place, &which) in order.iter().enumerate() {
            let expected = ["1", "3", "4.5"][which];
            assert_eq!(
                manifest.clips[place].start,
                time(expected),
                "order {order:?}, clip {}",
                written[which].0
            );
            assert_eq!(manifest.clips[place].label, format!("#{}", written[which].0));
        }
    }
}

#[test]
fn s2_t012_c2_all_twenty_four_orders_of_a_chain_of_four_with_a_follower_inside_another_clip_give_the_same_starts() {
    let written: Vec<(&str, &str, &str)> = vec![
        ("a", "0.5", "1"),
        ("b", "after:a+1", "1"),
        ("c", "after:b-0.25", "2"),
        ("d", "after:c", "1"),
    ];
    // a ends at 1.5; b starts 2.5, ends 3.5; c starts 3.25, ends 5.25; d starts 5.25.
    let expected = ["0.5", "2.5", "3.25", "5.25"];
    for order in permutations(&[0, 1, 2, 3]) {
        let clips: Vec<RawElement> = order
            .iter()
            .enumerate()
            .map(|(place, &which)| clip(written[which].0, written[which].1, Some(written[which].2), line(place)))
            .collect();
        let resolved = resolve(clips);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{order:?}");
        let manifest = resolved.manifest.unwrap();
        for (place, &which) in order.iter().enumerate() {
            assert_eq!(manifest.clips[place].start, time(expected[which]), "order {order:?}");
        }
    }
    // A follower written inside another clip, in a chain: the start is measured from the composition.
    let outer = clip("outer", "0", Some("9"), 4);
    let mut inner = clip("inner", "after:first+1", Some("1"), 6);
    inner.parent = Some(1);
    let first = clip("first", "1", Some("1"), 5);
    let last = clip("last", "after:inner", Some("1"), 7);
    let resolved = resolve(vec![outer, first, inner, last]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(starts(&resolved), [time("0"), time("1"), time("3"), time("4")]);
}

#[test]
fn s2_t012_c2_a_long_chain_resolves_and_the_depth_does_not_matter() {
    for count in [200usize, 3000] {
        // #c0 starts at 0 and lasts 1 s; #c(n) follows #c(n-1). Written in reverse order, the worst case.
        let mut clips = Vec::new();
        for place in 0..count {
            let n = count - 1 - place;
            let start = if n == 0 { "0".to_string() } else { format!("after:c{}", n - 1) };
            clips.push(clip(&format!("c{n}"), &start, Some("1"), line(place)));
        }
        let resolved = resolve_in(&(count + 1).to_string(), clips);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{count}");
        let manifest = resolved.manifest.unwrap();
        for place in 0..count {
            let n = count - 1 - place;
            assert_eq!(manifest.clips[place].start, time(&n.to_string()), "{count}: #c{n}");
        }
    }
}

// ---------------------------------------------------------------------
// c3: a cycle of two
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c3_two_clips_that_follow_each_other_are_one_error_with_its_own_code_that_names_both() {
    let resolved = resolve(vec![clip("a", "after:b", Some("1"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    let diagnostic = &resolved.diagnostics[0];
    assert_eq!(diagnostic.line, 4);
    assert_eq!(
        diagnostic.description,
        "The starts of #a and #b depend on each other in a circle: #a starts after #b and #b starts after #a, so none of them can be worked out."
    );
    assert_eq!(
        diagnostic.remedy,
        "Break the circle: write the start of one of the clips of the circle as a time in seconds, or make it start after a clip that is not in the circle."
    );
    assert_eq!(model::exit_code(&resolved.diagnostics), 1);
    assert!(resolved.manifest.is_none());
    // The offsets change nothing: the circle is the same circle.
    for (a, b) in [("after:b+1", "after:a-1"), ("after:b", "after:a+0.5"), ("after: b - 2", "after:a")] {
        let resolved = resolve(vec![clip("a", a, Some("1"), 4), clip("b", b, Some("1"), 5)]);
        assert_eq!(codes(&resolved), ["FS-E053"], "{a} {b}");
    }
}

#[test]
fn s2_t012_c3_the_circle_is_listed_from_the_clip_written_first_and_reported_on_its_line_whatever_the_order() {
    let resolved = resolve(vec![clip("b", "after:a", Some("1"), 4), clip("a", "after:b", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert_eq!(resolved.diagnostics[0].line, 4);
    assert!(
        resolved.diagnostics[0]
            .description
            .starts_with("The starts of #b and #a depend on each other in a circle: #b starts after #a and #a starts after #b,"),
        "{}",
        resolved.diagnostics[0].description
    );
    // Other clips written between them change nothing about the text; the same input gives the same text.
    let build = || {
        resolve(vec![
            clip("x", "0", Some("1"), 4),
            clip("b", "after:a", Some("1"), 5),
            clip("y", "after:x", Some("1"), 6),
            clip("a", "after:b", Some("1"), 7),
        ])
    };
    let (first, second) = (build(), build());
    assert_eq!(first.diagnostics[0].description, second.diagnostics[0].description);
    assert_eq!(first.diagnostics[0].line, 5);
}

// ---------------------------------------------------------------------
// c4: a cycle of three
// ---------------------------------------------------------------------

#[test]
fn s2_t012_c4_three_clips_in_a_circle_are_one_error_with_the_same_code_that_names_all_three_in_each_file_order() {
    // #one follows #three, #two follows #one, #three follows #two.
    let written = [("one", "after:three"), ("two", "after:one"), ("three", "after:two")];
    for order in permutations(&[0, 1, 2]) {
        let clips: Vec<RawElement> = order
            .iter()
            .enumerate()
            .map(|(place, &which)| clip(written[which].0, written[which].1, Some("1"), line(place)))
            .collect();
        let resolved = resolve(clips);
        assert_eq!(codes(&resolved), ["FS-E053"], "{order:?}");
        let diagnostic = &resolved.diagnostics[0];
        for (id, _) in written {
            assert!(
                diagnostic.description.contains(&format!("#{id}")),
                "{order:?}: {}",
                diagnostic.description
            );
        }
        // Reported on the line of the clip written first, and begins with it.
        assert_eq!(diagnostic.line, line(0), "{order:?}");
        let first = written[order[0]].0;
        assert!(
            diagnostic.description.starts_with(&format!("The starts of #{first}, ")),
            "{order:?}: {}",
            diagnostic.description
        );
        assert!(resolved.manifest.is_none());
    }
    // In the order of the fixture the text is this one.
    let resolved = resolve(vec![
        clip("one", "after:three", Some("1"), 4),
        clip("two", "after:one", Some("1"), 5),
        clip("three", "after:two", Some("1"), 6),
    ]);
    assert_eq!(
        resolved.diagnostics[0].description,
        "The starts of #one, #three and #two depend on each other in a circle: #one starts after #three, #three starts after #two and #two starts after #one, so none of them can be worked out."
    );
    assert_eq!(model::exit_code(&resolved.diagnostics), 1);
}

#[test]
fn s2_t012_c4_a_circle_of_four_names_all_four() {
    let resolved = resolve(vec![
        clip("a", "after:d", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
        clip("d", "after:c", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert!(resolved.diagnostics[0]
        .description
        .starts_with("The starts of #a, #d, #c and #b depend"));
}

// ---------------------------------------------------------------------
// The decisions: what else is listed, and what is not
// ---------------------------------------------------------------------

#[test]
fn s2_t012_a_clip_that_names_itself_is_still_fs_e049_and_is_not_a_circle() {
    let resolved = resolve(vec![clip("a", "after:a", Some("1"), 4)]);
    assert_eq!(codes(&resolved), ["FS-E049"]);
    let resolved = resolve(vec![clip("a", "after:a+1", Some("1"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E049"]);
}

#[test]
fn s2_t012_two_separate_circles_are_two_errors_each_naming_only_its_own_clips() {
    let resolved = resolve(vec![
        clip("a", "after:b", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:e", Some("1"), 6),
        clip("d", "after:c", Some("1"), 7),
        clip("e", "after:d", Some("1"), 8),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053", "FS-E053"]);
    assert_eq!(resolved.diagnostics[0].line, 4);
    assert_eq!(resolved.diagnostics[1].line, 6);
    let (first, second) = (&resolved.diagnostics[0].description, &resolved.diagnostics[1].description);
    assert!(
        first.contains("#a") && first.contains("#b") && !first.contains("#c") && !first.contains("#d") && !first.contains("#e"),
        "{first}"
    );
    assert!(
        second.contains("#c") && second.contains("#d") && second.contains("#e") && !second.contains("#a") && !second.contains("#b"),
        "{second}"
    );
}

#[test]
fn s2_t012_a_clip_that_follows_a_member_of_a_circle_is_not_named_and_has_no_error_of_its_own() {
    let resolved = resolve(vec![
        clip("a", "after:b", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("tail", "after:a", Some("1"), 6),
        clip("deeper", "after:tail+1", Some("1"), 7),
        clip("deepest", "after:deeper", Some("1"), 8),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    let said = &resolved.diagnostics[0].description;
    assert!(!said.contains("tail") && !said.contains("deeper"), "{said}");
    // The same when the follower is written first, so it is looked at first.
    let resolved = resolve(vec![
        clip("deeper", "after:tail", Some("1"), 4),
        clip("tail", "after:b", Some("1"), 5),
        clip("a", "after:b", Some("1"), 6),
        clip("b", "after:a", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert_eq!(resolved.diagnostics[0].line, 6);
    assert!(resolved.diagnostics[0].description.starts_with("The starts of #a and #b depend"));
    // A clip that does not depend on the circle still resolves; the project has no manifest all the same.
    let resolved = resolve(vec![
        clip("a", "after:b", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("free", "1", Some("1"), 6),
        clip("after_free", "after:free", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert!(resolved.manifest.is_none());
}

#[test]
fn s2_t012_a_clip_that_follows_a_clip_whose_start_has_another_fault_gets_no_error_of_its_own() {
    // (what the first clip writes, the fault it is listed with)
    let cases: Vec<(&str, &str)> = vec![
        ("after:b", "FS-E049"),
        ("after:nothing", "FS-E050"),
        ("after:z+abc", "FS-E051"),
        ("after:z-9", "FS-E052"),
        ("abc", "FS-E021"),
        ("-1", "FS-E022"),
    ];
    for (written, code) in cases {
        // `b` is the clip with the fault, `c` follows it, `d` follows `c`.
        let resolved = resolve(vec![
            clip("z", "1", Some("1"), 4),
            clip("b", written, Some("1"), 5),
            clip("c", "after:b", Some("1"), 6),
            clip("d", "after:c+0.5", Some("1"), 7),
        ]);
        assert_eq!(codes(&resolved), [code], "{written}");
        assert_eq!(resolved.diagnostics[0].line, 5, "{written}");
        assert!(resolved.manifest.is_none(), "{written}");
    }
    // A follower of a clip that is itself refused (a named clip with no written duration) is silent too.
    let resolved = resolve(vec![
        clip("a", "1", None, 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
    ]);
    assert_eq!(codes(&resolved), ["FS-U003"]);
    assert_eq!(resolved.diagnostics[0].line, 5);
}

#[test]
fn s2_t012_a_clip_of_a_circle_whose_offset_is_not_a_time_is_still_a_link_of_it_and_fs_e051_is_listed_too() {
    let resolved = resolve(vec![clip("a", "after:b+abc", Some("1"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E051", "FS-E053"]);
    assert_eq!(resolved.diagnostics[0].line, 4);
    assert_eq!(resolved.diagnostics[1].line, 4);
    assert!(resolved.diagnostics[1].description.starts_with("The starts of #a and #b depend"));
    // The bad offset on the clip written second: each fault on its own line.
    let resolved = resolve(vec![clip("a", "after:b", Some("1"), 4), clip("b", "after:a+abc", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E053", "FS-E051"]);
    assert_eq!((resolved.diagnostics[0].line, resolved.diagnostics[1].line), (4, 5));
    // An offset that is not a time on a clip that only leads into the circle is its own fault only.
    let resolved = resolve(vec![
        clip("a", "after:b", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:a+abc", Some("1"), 6),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053", "FS-E051"]);
}

#[test]
fn s2_t012_a_clip_of_a_circle_with_no_written_duration_is_named_and_nothing_else_is_listed_for_it() {
    let resolved = resolve(vec![clip("a", "after:b", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert!(resolved.diagnostics[0].description.starts_with("The starts of #a and #b depend"));
    assert!(resolved.manifest.is_none());
    let resolved = resolve(vec![
        clip("a", "after:c", Some("1"), 4),
        clip("b", "after:a", None, 5),
        clip("c", "after:b", None, 6),
        clip("d", "after:b", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    // A circle never produces a manifest, so it never renders.
    assert!(resolved.manifest.is_none());
}

#[test]
fn s2_t012_a_circle_reached_through_an_id_that_holds_a_hyphen_or_a_sign_is_found_the_same_way() {
    let resolved = resolve(vec![
        clip("scene-1", "after:scene-2", Some("1"), 4),
        clip("scene-2", "after:scene-1+0.5", Some("1"), 5),
    ]);
    assert_eq!(codes(&resolved), ["FS-E053"]);
    assert!(resolved.diagnostics[0].description.contains("#scene-1") && resolved.diagnostics[0].description.contains("#scene-2"));
}

#[test]
fn s2_t012_the_new_code_is_listed_only_for_a_circle_and_a_chain_without_a_circle_has_no_diagnostic() {
    // A chain that ends in the clip it began with by way of an absolute clip is not a circle.
    let resolved = resolve(vec![
        clip("a", "1", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
        clip("d", "after:a", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(starts(&resolved), [time("1"), time("2"), time("3"), time("2")]);
}
