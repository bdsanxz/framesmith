//! Sprint 2, ticket T013, at the level of the resolver: the start of a clip that follows a clip with no
//! written duration (a still image, a clip that lasts to the end of the composition) and the error for an
//! end that cannot be worked out. The page and the host resolve with this code.

use model::{Draft, RawAttr, RawDoc, RawElement, RawMotion, Resolved};
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

/// The composition, with the given attributes besides its name and size (3 on line 3).
fn composition(attrs: &[(&str, &str)]) -> RawElement {
    let mut all = vec![("data-fs-composition", "main"), ("data-fs-size", "64x36")];
    all.extend_from_slice(attrs);
    element("div", None, None, 3, &all)
}

/// A `div` clip with an id, a start, and a duration when one is given; its line is given.
fn clip(id: &str, start: &str, duration: Option<&str>, line: u32) -> RawElement {
    let mut attrs = vec![("data-fs-clip", ""), ("data-fs-start", start)];
    if let Some(duration) = duration {
        attrs.push(("data-fs-duration", duration));
    }
    element("div", Some(id), Some(0), line, &attrs)
}

/// A still image clip, with no duration unless one is given.
fn image(id: &str, start: &str, duration: Option<&str>, line: u32) -> RawElement {
    let mut attrs = vec![("data-fs-clip", ""), ("data-fs-start", start)];
    if let Some(duration) = duration {
        attrs.push(("data-fs-duration", duration));
    }
    element("img", Some(id), Some(0), line, &attrs)
}

fn resolve_in(composition_attrs: &[(&str, &str)], clips: Vec<RawElement>, motion: Vec<RawMotion>) -> Resolved {
    let mut elements = vec![composition(composition_attrs)];
    elements.extend(clips);
    Draft::read(&RawDoc { elements, motion }, "index.html").finish(&mut |_| Ok(vec![7]))
}

/// A composition with no length written and no motion.
fn unwritten(clips: Vec<RawElement>) -> Resolved {
    resolve_in(&[], clips, vec![])
}

/// A composition of the given length (written) and no motion.
fn written(length: &str, clips: Vec<RawElement>) -> Resolved {
    resolve_in(&[("data-fs-duration", length)], clips, vec![])
}

/// A composition with no length written and one move that ends at `end` seconds.
fn moving(end: &str, clips: Vec<RawElement>) -> Resolved {
    let block = format!(r##"[ {{ "target": "#x", "to": {{ "x": 1 }}, "at": 0, "duration": {end} }} ]"##);
    resolve_in(&[], clips, vec![RawMotion { text: block, line: 20 }])
}

fn codes(resolved: &Resolved) -> Vec<&str> {
    resolved.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn time(text: &str) -> Time {
    Time::parse(text).unwrap()
}

// ---------------------------------------------------------------------
// c1: a still image with no duration lasts 3 seconds
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c1_a_clip_that_follows_a_still_image_starts_at_the_image_start_plus_three_seconds() {
    let resolved = written("10", vec![image("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].start, time("4"));
    assert_eq!(manifest.clips[1].relative_start.as_deref(), Some("after:a"));
    // The image's duration is the three seconds, marked as worked out ("filled in" for `check`).
    assert_eq!(manifest.clips[0].duration, time("3"));
    assert!(!manifest.clips[0].duration_written);
    // The follower's own duration is the one written.
    assert!(manifest.clips[1].duration_written);
}

#[test]
fn s2_t013_c1_the_offset_is_added_to_the_end_of_the_image() {
    for (written_start, expected) in [
        ("after:a", "4"),
        ("after:a+0.5", "4.5"),
        ("after:a - 0.5", "3.5"),
        ("after:a+0", "4"),
        ("after:a-4", "0"),
    ] {
        let resolved = written("10", vec![image("a", "1", None, 4), clip("b", written_start, Some("1"), 5)]);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{written_start}");
        assert_eq!(resolved.manifest.unwrap().clips[1].start, time(expected), "{written_start}");
    }
    // The sum is exact: 1.1 + 3 + 0.2 is 4.3, and a start before zero is an error, as for any clip.
    let resolved = written("10", vec![image("a", "1.1", None, 4), clip("b", "after:a+0.2", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().clips[1].start, time("4.3"));
    let before = written("10", vec![image("a", "1", None, 4), clip("b", "after:a-4.5", Some("1"), 5)]);
    assert_eq!(codes(&before), ["FS-E052"]);
    assert!(before.diagnostics[0].description.contains("#a ends at 4 s"));
    assert!(before.manifest.is_none());
}

#[test]
fn s2_t013_c1_an_image_that_has_a_duration_written_keeps_it_and_the_follower_follows_that() {
    let resolved = written("10", vec![image("a", "1", Some("2"), 4), clip("b", "after:a", Some("1"), 5)]);
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].start, time("3"));
    assert!(manifest.clips[0].duration_written);
}

#[test]
fn s2_t013_c1_a_follower_of_an_image_takes_part_in_working_the_length_out() {
    // No length written, no motion: the latest end among the clips with an end of their own. The image
    // ends at 4 s and the follower, from 4 s for 1 s, at 5 s.
    let resolved = unwritten(vec![image("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.length, time("5"));
    assert!(!manifest.length_written);
    // With motion the length is the end of the motion, and the follower is cut like any clip.
    let resolved = moving("4.5", vec![image("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().length, time("4.5"));
}

#[test]
fn s2_t013_c1_a_chain_through_an_image_is_worked_out_link_by_link_in_any_order() {
    // #c follows #b, which follows the image #a (1 s to 4 s): #b from 4 s for 1.5 s, #c from 5.5 s.
    let in_order = [
        image("a", "1", None, 4),
        clip("b", "after:a", Some("1.5"), 5),
        clip("c", "after:b", Some("1"), 6),
    ];
    let reversed = [
        clip("c", "after:b", Some("1"), 4),
        clip("b", "after:a", Some("1.5"), 5),
        image("a", "1", None, 6),
    ];
    let a = written("10", in_order.to_vec()).manifest.unwrap();
    assert_eq!((a.clips[1].start, a.clips[2].start), (time("4"), time("5.5")));
    let b = written("10", reversed.to_vec()).manifest.unwrap();
    assert_eq!((b.clips[0].start, b.clips[1].start), (time("5.5"), time("4")));
    // An image that follows an image: the second starts at 4 s and ends at 7 s; the follower of it at 7 s.
    let images = written(
        "10",
        vec![
            image("a", "1", None, 4),
            image("b", "after:a", None, 5),
            clip("c", "after:b", Some("1"), 6),
        ],
    )
    .manifest
    .unwrap();
    assert_eq!((images.clips[1].start, images.clips[2].start), (time("4"), time("7")));
}

// ---------------------------------------------------------------------
// c2: a clip that lasts to the end of the composition
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c2_a_follower_of_a_clip_that_lasts_to_the_end_starts_at_the_end_and_gets_the_warning() {
    for (key, resolved) in [
        (
            "written",
            written("4", vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]),
        ),
        (
            "worked out from the end of the motion",
            moving("4", vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]),
        ),
        (
            "worked out from another clip",
            unwritten(vec![
                clip("a", "1", None, 4),
                clip("b", "after:a", Some("1"), 5),
                clip("c", "0", Some("4"), 6),
            ]),
        ),
    ] {
        assert_eq!(codes(&resolved), ["FS-W001"], "{key}");
        assert_eq!(resolved.diagnostics[0].line, 5, "{key}");
        assert!(resolved.diagnostics[0].description.contains("#b starts at 4 s"), "{key}");
        let manifest = resolved.manifest.expect(key);
        assert_eq!(manifest.length, time("4"), "{key}");
        assert_eq!(manifest.clips[1].start, manifest.length, "{key}");
        // The named clip lasts to the end: 1 s to 4 s.
        assert_eq!(
            (manifest.clips[0].start, manifest.clips[0].duration),
            (time("1"), time("3")),
            "{key}"
        );
        assert!(!manifest.clips[0].duration_written, "{key}");
    }
}

#[test]
fn s2_t013_c2_the_follower_does_not_lengthen_the_composition_it_follows_the_end_of() {
    // #c ends at 4 s; #b would end at 5 s if it counted, and it does not: the length stays 4 s.
    let resolved = unwritten(vec![
        clip("c", "0", Some("4"), 4),
        clip("a", "1", None, 5),
        clip("b", "after:a", Some("1"), 6),
    ]);
    assert_eq!(resolved.manifest.unwrap().length, time("4"));
    // The order in which the clips are written changes nothing.
    let resolved = unwritten(vec![
        clip("b", "after:a", Some("1"), 4),
        clip("a", "1", None, 5),
        clip("c", "0", Some("4"), 6),
    ]);
    let manifest = resolved.manifest.unwrap();
    assert_eq!((manifest.length, manifest.clips[0].start), (time("4"), time("4")));
}

#[test]
fn s2_t013_c2_the_offset_moves_the_start_from_the_end_and_a_negative_offset_can_show() {
    for (written_start, start, warns) in [
        ("after:a", "4", true),
        ("after:a+0.5", "4.5", true),
        ("after:a-1", "3", false),
        ("after:a - 4", "0", false),
        ("after:a-0.5", "3.5", false),
    ] {
        let resolved = written("4", vec![clip("a", "1", None, 4), clip("b", written_start, Some("1"), 5)]);
        assert_eq!(codes(&resolved), if warns { vec!["FS-W001"] } else { vec![] }, "{written_start}");
        assert_eq!(resolved.manifest.unwrap().clips[1].start, time(start), "{written_start}");
    }
    let before = written("4", vec![clip("a", "1", None, 4), clip("b", "after:a-4.5", Some("1"), 5)]);
    assert_eq!(codes(&before), ["FS-E052"]);
    assert!(before.diagnostics[0].description.contains("#a ends at 4 s"));
}

#[test]
fn s2_t013_c2_a_named_clip_that_starts_at_or_after_the_end_ends_where_it_starts() {
    // #a never shows: its duration is 0 and its end is its start, 6 s; #b follows that end.
    let resolved = written("4", vec![clip("a", "6", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-W001", "FS-W001"]);
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[0].duration, time("0"));
    assert_eq!(manifest.clips[1].start, time("6"));
}

#[test]
fn s2_t013_c2_a_chain_through_a_clip_that_lasts_to_the_end_is_worked_out_from_the_length() {
    // #b follows #a (to the end, 4 s) with -2: from 2 s for 1.5 s; #c follows #b from 3.5 s.
    let clips = vec![
        clip("a", "1", None, 4),
        clip("b", "after:a-2", Some("1.5"), 5),
        clip("c", "after:b", Some("1"), 6),
    ];
    let manifest = written("4", clips.clone()).manifest.unwrap();
    assert_eq!((manifest.clips[1].start, manifest.clips[2].start), (time("2"), time("3.5")));
    // The same with the clips written the other way round.
    let mut reversed = clips;
    reversed.reverse();
    let manifest = written("4", reversed).manifest.unwrap();
    assert_eq!((manifest.clips[1].start, manifest.clips[0].start), (time("2"), time("3.5")));
    // A follower that is itself a clip with no duration lasts to the end from where it starts.
    let resolved = written(
        "4",
        vec![
            clip("a", "1", None, 4),
            clip("b", "after:a-2", None, 5),
            clip("c", "after:b", Some("1"), 6),
        ],
    );
    let manifest = resolved.manifest.unwrap();
    assert_eq!((manifest.clips[1].start, manifest.clips[1].duration), (time("2"), time("2")));
    assert_eq!(manifest.clips[2].start, time("4"));
}

#[test]
fn s2_t013_c2_a_clip_that_follows_an_image_that_follows_the_end_is_worked_out_too() {
    // #i follows #a (to the end, 6 s) with -1: from 5 s, an image of 3 s, to 8 s (cut at 6 s); #c at 8 s.
    let resolved = written(
        "6",
        vec![
            clip("a", "1", None, 4),
            image("i", "after:a-1", None, 5),
            clip("c", "after:i", Some("1"), 6),
        ],
    );
    assert_eq!(codes(&resolved), ["FS-W001"]);
    let manifest = resolved.manifest.unwrap();
    assert_eq!((manifest.clips[1].start, manifest.clips[2].start), (time("5"), time("8")));
}

// ---------------------------------------------------------------------
// c3: an end that cannot be worked out
// ---------------------------------------------------------------------

#[test]
fn s2_t013_c3_a_follower_of_a_clip_whose_end_cannot_be_worked_out_is_an_error_naming_both_clips() {
    let resolved = unwritten(vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E025", "FS-E054"]);
    assert!(resolved.manifest.is_none());
    let error = &resolved.diagnostics[1];
    assert_eq!(error.line, 5);
    assert_eq!(error.file, "index.html");
    // Both clips are named, and the fix says what to write.
    assert!(
        error.description.starts_with("The clip #b starts after #a, which has no duration"),
        "{}",
        error.description
    );
    assert!(error
        .description
        .contains("none is written, and there is no motion and no clip with its own end"));
    for wanted in [
        "Write a duration on #a",
        "a length on the composition",
        "write the start of #b as a time in seconds",
    ] {
        assert!(error.remedy.contains(wanted), "{}", error.remedy);
    }
    // It is an error, not a refusal.
    assert_eq!(model::exit_code(&resolved.diagnostics), 1);
}

#[test]
fn s2_t013_c3_the_code_is_the_same_with_an_offset_and_with_a_follower_written_first() {
    for (key, clips) in [
        ("offset", vec![clip("a", "1", None, 4), clip("b", "after:a + 0.5", Some("1"), 5)]),
        ("negative", vec![clip("a", "1", None, 4), clip("b", "after:a-0.5", Some("1"), 5)]),
        ("follower first", vec![clip("b", "after:a", Some("1"), 4), clip("a", "1", None, 5)]),
    ] {
        let resolved = unwritten(clips);
        let mut found = codes(&resolved);
        found.sort();
        assert_eq!(found, ["FS-E025", "FS-E054"], "{key}");
    }
}

#[test]
fn s2_t013_c3_a_clip_with_no_id_cannot_be_named_so_that_is_the_missing_clip_and_not_this_error() {
    let plain = element("div", None, Some(0), 5, &[("data-fs-clip", ""), ("data-fs-start", "1")]);
    let resolved = unwritten(vec![plain, clip("b", "after:a", Some("1"), 6)]);
    assert_eq!(codes(&resolved), ["FS-E025", "FS-E050"]);
}

#[test]
fn s2_t013_c3_a_follower_of_a_follower_of_such_a_clip_gets_no_error_of_its_own() {
    // #b is the clip that follows the unknown end (FS-E054); #c follows #b and is silent, so is #d.
    let resolved = unwritten(vec![
        clip("a", "1", None, 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
        clip("d", "after:c+1", Some("1"), 7),
    ]);
    assert_eq!(codes(&resolved), ["FS-E025", "FS-E054"]);
    assert_eq!(resolved.diagnostics[1].line, 5);
    // Two clips that follow the same clip are two errors, one for each.
    let resolved = unwritten(vec![
        clip("a", "1", None, 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:a+1", Some("1"), 6),
    ]);
    assert_eq!(codes(&resolved), ["FS-E025", "FS-E054", "FS-E054"]);
    assert!(resolved.diagnostics[1].description.starts_with("The clip #b starts after #a"));
    assert!(resolved.diagnostics[2].description.starts_with("The clip #c starts after #a"));
}

#[test]
fn s2_t013_c3_writing_what_the_fix_says_makes_the_project_pass() {
    // A duration on the named clip.
    let resolved = unwritten(vec![clip("a", "1", Some("2"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(resolved.manifest.unwrap().clips[1].start, time("3"));
    // A length on the composition.
    let resolved = written("6", vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().clips[1].start, time("6"));
    // The follower written as a time.
    let resolved = unwritten(vec![clip("a", "1", None, 4), clip("b", "2", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().length, time("3"));
    // Another clip with an end of its own, which gives the composition its length.
    let resolved = unwritten(vec![
        clip("a", "1", None, 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "0", Some("2"), 6),
    ]);
    assert_eq!(codes(&resolved), ["FS-W001"]);
}

#[test]
fn s2_t013_c3_a_length_that_is_wrong_is_reported_once_and_the_follower_is_not_reported_again() {
    // The length is written wrongly: its own error is listed, and nothing says the end cannot be worked out.
    let resolved = resolve_in(
        &[("data-fs-duration", "soon")],
        vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)],
        vec![],
    );
    assert_eq!(codes(&resolved), ["FS-E009"]);
    // A clip that follows a clip whose own start is wrong is silent, as before.
    let resolved = unwritten(vec![clip("a", "-1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E022", "FS-E025"]);
}

#[test]
fn s2_t013_c3_the_error_is_given_to_nothing_else_and_sits_in_the_resolver_alone() {
    let source = include_str!("../src/resolve.rs");
    assert_eq!(source.matches("\"FS-E054\"").count(), 1);
}
