//! Sprint 2, ticket T011, at the level of the resolver: how the offset after `after:<clip id>` is spelled
//! and read, the exact sum, the two faults (an offset that is not a time; a start before zero), and what
//! stays refused. The page and the host resolve with this code, so these rules hold in both.

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
    resolve_in("10", clips)
}

fn codes(resolved: &Resolved) -> Vec<&str> {
    resolved.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn time(text: &str) -> Time {
    Time::parse(text).unwrap()
}

/// `#intro` from 0 for 1 s, then a follower `#b` written `written`, plus the extra clips (also 1 s from 0).
fn follower(written: &str, ids: &[&str]) -> Resolved {
    let mut clips = vec![clip("intro", "0", Some("1"), 4)];
    for (i, id) in ids.iter().enumerate() {
        clips.push(clip(id, "0", Some("1"), 5 + i as u32));
    }
    clips.push(clip("b", written, Some("1"), 20));
    resolve(clips)
}

/// The start of the follower (the last clip) when it resolved.
fn start_of_follower(resolved: &Resolved) -> Time {
    resolved.manifest.as_ref().expect("a manifest").clips.last().unwrap().start
}

// ---------------------------------------------------------------------
// c1: the sum
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c1_a_plus_offset_is_added_to_the_end_of_the_named_clip_and_a_minus_offset_is_taken_from_it() {
    // #a ends at 1 + 2 = 3 s.
    let resolved = resolve(vec![
        clip("a", "1", Some("2"), 4),
        clip("p", "after:a+0.5", Some("1"), 5),
        clip("m", "after:a-0.5", Some("1"), 6),
    ]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].start, time("3.5"));
    assert_eq!(manifest.clips[2].start, time("2.5"));
    // The form is carried as written.
    assert_eq!(manifest.clips[1].relative_start.as_deref(), Some("after:a+0.5"));
    assert_eq!(manifest.clips[2].relative_start.as_deref(), Some("after:a-0.5"));
    // The named clip is not changed.
    assert_eq!(manifest.clips[0].start, time("1"));
    assert_eq!(manifest.clips[0].duration, time("2"));
}

#[test]
fn s2_t011_c1_the_spelling_of_an_offset_spaces_around_the_sign_a_bare_point_and_zero() {
    // (written, the start of the follower; #intro ends at 1 s)
    let cases = [
        ("after:intro+0.5", "1.5"),
        ("after:intro-0.5", "0.5"),
        ("after:intro +0.5", "1.5"),
        ("after:intro+ 0.5", "1.5"),
        ("after:intro + 0.5", "1.5"),
        ("after:intro - 0.5", "0.5"),
        ("after:intro -0.5", "0.5"),
        ("after:intro- 0.5", "0.5"),
        ("after:  intro   +   0.5  ", "1.5"),
        ("after:intro+2", "3"),
        ("after:intro+.5", "1.5"),
        ("after:intro-.5", "0.5"),
        ("after:intro+0", "1"),
        ("after:intro-0", "1"),
        ("after:intro+0.0", "1"),
        ("after:intro-0.000000000", "1"),
        ("after:intro+0.123456789", "1.123456789"),
        ("after:intro-1", "0"),
        ("after:intro-1.0", "0"),
    ];
    for (written, expected) in cases {
        let resolved = follower(written, &[]);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{written}");
        assert_eq!(start_of_follower(&resolved), time(expected), "{written}");
    }
}

#[test]
fn s2_t011_c1_the_written_form_is_kept_as_written_with_the_spaces_at_its_ends_removed() {
    let resolved = follower("   after: intro + 0.5  ", &[]);
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].relative_start.as_deref(), Some("after: intro + 0.5"));
}

#[test]
fn s2_t011_c1_the_named_clip_may_come_later_in_the_file_and_may_stand_inside_another_clip() {
    let resolved = resolve(vec![clip("b", "after:a-0.25", Some("1"), 4), clip("a", "2", Some("0.5"), 5)]);
    assert_eq!(resolved.manifest.unwrap().clips[0].start, time("2.25"));
    let outer = element(
        "div",
        Some("outer"),
        Some(0),
        4,
        &[("data-fs-clip", ""), ("data-fs-start", "0"), ("data-fs-duration", "9")],
    );
    let inner = element(
        "div",
        Some("inner"),
        Some(1),
        5,
        &[("data-fs-clip", ""), ("data-fs-start", "1"), ("data-fs-duration", "1.5")],
    );
    let resolved = resolve(vec![outer, inner, clip("b", "after:inner+1", Some("1"), 6)]);
    assert_eq!(resolved.manifest.unwrap().clips[2].start, time("3.5"));
}

// ---------------------------------------------------------------------
// c2: the sum is exact
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c2_the_sum_is_an_exact_fraction_so_a_tenth_and_two_tenths_make_exactly_three_tenths() {
    // 0.1 + 0.2 in binary floating point is 0.30000000000000004 (a hair above 0.3).
    let resolved = resolve(vec![clip("a", "0", Some("0.1"), 4), clip("b", "after:a+0.2", Some("1"), 5)]);
    let start = resolved.manifest.unwrap().clips[1].start;
    assert_eq!(start, Time::new(3, 10));
    // At 10 frames a second the exact 0.3 s is frame 3; a floating-point sum rounded up as it stands
    // would give frame 4 (with the tolerance of the frame grid rule it also gives frame 3).
    assert_eq!(start.grid_frame(10), 3);
}

#[test]
fn s2_t011_c2_an_offset_of_a_third_of_a_frame_written_with_nine_decimals_is_added_as_written() {
    // 0.011111111 s at 30 frames a second is 0.33333333 of a frame: 11111111 / 33333333.33.. exactly
    // 0.011111111 = 11111111 / 1_000_000_000; times 30 = 333333330 / 1_000_000_000 = 0.33333333 frame.
    let offset = "0.011111111";
    // On a frame time (1 s = frame 30): the sum is 30.33333333 frames, which rounds up to 31.
    let on = resolve(vec![
        clip("a", "0", Some("1"), 4),
        clip("b", &format!("after:a+{offset}"), Some("1"), 5),
    ]);
    let start = on.manifest.unwrap().clips[1].start;
    assert_eq!(start, Time::new(1_011_111_111, 1_000_000_000));
    assert_eq!(start.grid_frame(30), 31);
    // The same with a minus: 0.988888889 s is 29.66666667 frames, rounds up to 30.
    let minus = resolve(vec![
        clip("a", "0", Some("1"), 4),
        clip("b", &format!("after:a-{offset}"), Some("1"), 5),
    ]);
    let start = minus.manifest.unwrap().clips[1].start;
    assert_eq!(start, Time::new(988_888_889, 1_000_000_000));
    assert_eq!(start.grid_frame(30), 30);
    // Just below a frame time (0.999999999 s) plus one third of a frame is 30.3 frames: frame 31.
    let below = resolve(vec![
        clip("a", "0", Some("0.999999999"), 4),
        clip("b", &format!("after:a+{offset}"), Some("1"), 5),
    ]);
    let start = below.manifest.unwrap().clips[1].start;
    assert_eq!(start, Time::new(1_011_111_110, 1_000_000_000));
    assert_eq!(start.grid_frame(30), 31);
    // Just below a frame time, taken back by that third of a frame: 29.6666 frames, frame 30.
    let below_minus = resolve(vec![
        clip("a", "0", Some("0.999999999"), 4),
        clip("b", &format!("after:a-{offset}"), Some("1"), 5),
    ]);
    assert_eq!(below_minus.manifest.unwrap().clips[1].start.grid_frame(30), 30);
}

#[test]
fn s2_t011_c2_a_sum_exactly_on_the_edge_of_the_tolerance_is_taken_as_the_whole_frame() {
    // At 10 frames a second, 0.4 s plus 0.0001 s is 4.001 frames: exactly 0.001 above frame 4, which the
    // frame grid rule includes ("within 0.001 ... includes exactly 0.001"), so frame 4. In binary floating
    // point 0.4 + 0.0001 times 10 is a hair above 4.001, which is not within the tolerance: frame 5.
    let resolved = resolve(vec![clip("a", "0", Some("0.4"), 4), clip("b", "after:a+0.0001", Some("1"), 5)]);
    let start = resolved.manifest.unwrap().clips[1].start;
    assert_eq!(start, Time::new(4001, 10000));
    assert_eq!(start.grid_frame(10), 4);
    // The same from the other side: 0.4001 s less 0.0001 s is 4 frames exactly.
    let resolved = resolve(vec![clip("a", "0", Some("0.4001"), 4), clip("b", "after:a-0.0001", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().clips[1].start, Time::new(2, 5));
    // One part in ten thousand more is past the tolerance: frame 5.
    let resolved = resolve(vec![clip("a", "0", Some("0.4"), 4), clip("b", "after:a+0.0002", Some("1"), 5)]);
    assert_eq!(resolved.manifest.unwrap().clips[1].start.grid_frame(10), 5);
}

// ---------------------------------------------------------------------
// The four steps: step 1 first
// ---------------------------------------------------------------------

#[test]
fn s2_t011_step_one_a_text_that_is_exactly_a_clip_id_names_that_clip_and_is_not_an_offset() {
    for id in ["scene-2", "intro-0.5", "intro+0.5", "a - 1", "x-1.5"] {
        let resolved = follower(&format!("after:{id}"), &[id]);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{id}");
        assert_eq!(start_of_follower(&resolved), time("1"), "{id}: the end of the clip with that id");
    }
}

#[test]
fn s2_t011_step_one_then_two_the_same_text_is_a_clip_when_it_is_there_and_an_offset_when_it_is_not() {
    // `scene-2` present: the clip `scene-2` (ends at 1 s). Only `scene` present: `scene` minus 2 s, which
    // is before zero. Both present: the clip `scene-2` wins. Neither: an id no clip has.
    let both = resolve(vec![
        clip("scene", "0", Some("5"), 4),
        clip("scene-2", "0", Some("1"), 5),
        clip("b", "after:scene-2", Some("1"), 6),
    ]);
    assert_eq!(codes(&both), Vec::<&str>::new());
    assert_eq!(both.manifest.unwrap().clips[2].start, time("1"));

    let only_scene = resolve(vec![clip("scene", "0", Some("5"), 4), clip("b", "after:scene-2", Some("1"), 5)]);
    assert_eq!(codes(&only_scene), Vec::<&str>::new());
    assert_eq!(only_scene.manifest.unwrap().clips[1].start, time("3"));

    let neither = resolve(vec![clip("x", "0", Some("5"), 4), clip("b", "after:scene-2", Some("1"), 5)]);
    assert_eq!(codes(&neither), ["FS-E050"]);
    assert_eq!(
        neither.diagnostics[0].description,
        "The clip #b starts after \"scene-2\", but no clip with the id \"scene-2\" is in this composition; \
         read as the clip \"scene\" and the offset \"-2\", no clip with the id \"scene\" is in it either; \
         only an element marked data-fs-clip can be named."
    );
    assert_eq!(neither.diagnostics[0].line, 5);
}

#[test]
fn s2_t011_a_hyphen_followed_by_something_that_is_not_a_number_is_a_name_and_never_an_offset() {
    for (written, shown) in [
        ("after:intro-abc", "intro-abc"),
        ("after:intro-2b", "intro-2b"),
        ("after:intro-0.5s", "intro-0.5s"),
        ("after:intro-", "intro-"),
        ("after:intro 0.5", "intro 0.5"),
        ("after:intro--abc", "intro--abc"),
        ("after:+1", "+1"),
        ("after:-1", "-1"),
        ("after:+-1", "+-1"),
        ("after:a b", "a b"),
    ] {
        let resolved = follower(written, &[]);
        assert_eq!(codes(&resolved), ["FS-E050"], "{written}");
        assert_eq!(
            resolved.diagnostics[0].description,
            format!(
                "The clip #b starts after \"{shown}\", but no clip with the id \"{shown}\" is in this composition; only an element marked data-fs-clip can be named."
            ),
            "{written}"
        );
        assert!(resolved.manifest.is_none(), "{written}");
    }
}

// ---------------------------------------------------------------------
// c3: an offset that is not a time
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c3_an_offset_that_is_not_a_time_is_an_error_with_its_own_code_that_shows_what_was_written() {
    // (written, the offset shown)
    let cases = [
        ("after:intro+abc", "+abc"),
        ("after:intro+", "+"),
        ("after:intro+1.2.3", "+1.2.3"),
        ("after:intro-1.2.3", "-1.2.3"),
        ("after:intro+0.5s", "+0.5s"),
        ("after:intro+1e3", "+1e3"),
        ("after:intro+1e-3", "+1e-3"),
        ("after:intro+1.", "+1."),
        ("after:intro-1.", "-1."),
        ("after:intro+0,5", "+0,5"),
        ("after:intro+0. 5", "+0. 5"),
        ("after:intro+1 2", "+1 2"),
        ("after:intro+0.1234567891", "+0.1234567891"),
        ("after:intro-0.1234567891", "-0.1234567891"),
        ("after:intro+1234567890", "+1234567890"),
        ("after:intro+-0.5", "+-0.5"),
        ("after:intro--0.5", "--0.5"),
        ("after:intro++1", "++1"),
        ("after:intro-+1", "-+1"),
        ("after:intro + - 1", "+ - 1"),
        ("after:intro+.", "+."),
    ];
    for (written, offset) in cases {
        let resolved = follower(written, &[]);
        assert_eq!(codes(&resolved), ["FS-E051"], "{written}");
        let item = &resolved.diagnostics[0];
        assert_eq!(item.line, 20, "{written}");
        assert_eq!(
            item.description,
            format!("The offset \"{offset}\" in the start \"{written}\" of the clip #b is not a time in seconds."),
            "{written}"
        );
        assert!(item.remedy.contains("after:intro+0.5"), "{written}: {}", item.remedy);
        assert!(resolved.manifest.is_none(), "{written}");
    }
}

#[test]
fn s2_t011_c3_when_the_offset_is_not_a_time_and_the_base_is_no_clip_both_faults_are_listed() {
    let resolved = follower("after:nothing+abc", &[]);
    assert_eq!(codes(&resolved), ["FS-E051", "FS-E050"]);
    assert_eq!(
        resolved.diagnostics[1].description,
        "The clip #b starts after \"nothing\" (the part of \"after:nothing+abc\" before the offset), but no clip with the id \"nothing\" is in this composition; only an element marked data-fs-clip can be named."
    );
    // A valid offset after a base that is no clip is the one fault, FS-E050.
    let resolved = follower("after:nothing+1", &[]);
    assert_eq!(codes(&resolved), ["FS-E050"]);
}

#[test]
fn s2_t011_c3_the_codes_of_the_two_new_faults_differ_from_each_other_and_from_the_older_ones() {
    let not_time = follower("after:intro+abc", &[]);
    let before_zero = follower("after:intro-2", &[]);
    let (a, b) = (&not_time.diagnostics[0].code, &before_zero.diagnostics[0].code);
    assert_eq!((a.as_str(), b.as_str()), ("FS-E051", "FS-E052"));
    for older in ["FS-E021", "FS-E022", "FS-E049", "FS-E050", "FS-U003"] {
        assert_ne!(a, older);
        assert_ne!(b, older);
    }
}

// ---------------------------------------------------------------------
// c4: a start before zero
// ---------------------------------------------------------------------

#[test]
fn s2_t011_c4_a_start_before_zero_is_an_error_that_gives_the_resolved_time_and_the_clip_is_not_moved() {
    // #intro ends at 1 s; the offset -1.5 gives -0.5 s.
    let resolved = follower("after:intro-1.5", &[]);
    assert_eq!(codes(&resolved), ["FS-E052"]);
    let item = &resolved.diagnostics[0];
    assert_eq!(item.line, 20);
    assert_eq!(
        item.description,
        "The clip #b starts at -0.5 s (\"after:intro-1.5\", and #intro ends at 1 s), before the start of the composition."
    );
    assert_eq!(
        item.remedy,
        "Use an offset of -1 s or later, or write the start as a time of 0 or more in seconds. The clip is not moved to 0."
    );
    // No manifest: the clip is not given a start of 0.
    assert!(resolved.manifest.is_none());
}

#[test]
fn s2_t011_c4_a_start_of_exactly_zero_is_accepted_and_a_start_just_below_zero_is_not() {
    let zero = follower("after:intro-1", &[]);
    assert_eq!(codes(&zero), Vec::<&str>::new());
    assert_eq!(start_of_follower(&zero), Time::ZERO);
    let just_below = follower("after:intro-1.000000001", &[]);
    assert_eq!(codes(&just_below), ["FS-E052"]);
    assert!(
        just_below.diagnostics[0].description.contains("starts at -0.000000001 s"),
        "{}",
        just_below.diagnostics[0].description
    );
}

#[test]
fn s2_t011_c4_the_sprint_1_fault_for_a_written_negative_start_keeps_its_code() {
    let resolved = resolve(vec![clip("a", "-0.5", Some("1"), 4)]);
    assert_eq!(codes(&resolved), ["FS-E022"]);
}

// ---------------------------------------------------------------------
// What stays refused or reported otherwise
// ---------------------------------------------------------------------

#[test]
fn s2_t011_a_clip_that_names_itself_with_an_offset_is_still_a_clip_that_names_itself() {
    let resolved = resolve(vec![clip("b", "after:b+0.5", Some("1"), 4)]);
    assert_eq!(codes(&resolved), ["FS-E049"]);
    assert!(resolved.diagnostics[0].description.contains("(\"after:b+0.5\")"));
}

#[test]
fn s2_t011_a_chain_is_resolved_a_circle_and_a_nested_composition_are_not_and_a_named_clip_with_no_written_duration_takes_the_offset() {
    // A chain no longer waits (ticket T012): the offset on a link is added to the end of the clip before
    // it, and a link that would start before zero is FS-E052 for that link.
    for (written, start) in [("after:b", "3"), ("after:b+0.5", "3.5"), ("after:b-0.5", "2.5")] {
        let chain = resolve(vec![
            clip("a", "1", Some("1"), 4),
            clip("b", "after:a", Some("1"), 5),
            clip("c", written, Some("1"), 6),
        ]);
        assert_eq!(codes(&chain), Vec::<&str>::new(), "{written}");
        assert_eq!(chain.manifest.unwrap().clips[2].start, time(start), "{written}");
    }
    // A chain whose last offset would go before zero: the fault is the one link's, and nothing is moved.
    let chain = resolve(vec![
        clip("a", "1", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b-99", Some("1"), 6),
    ]);
    assert_eq!(codes(&chain), ["FS-E052"]);
    // A cycle of two is one error (ticket T012).
    let cycle = resolve(vec![clip("a", "after:b+1", Some("1"), 4), clip("b", "after:a-1", Some("1"), 5)]);
    assert_eq!(codes(&cycle), ["FS-E053"]);
    // A named clip with no written duration, a still image included (ticket T013): the offset is added to
    // its worked-out end. The composition lasts 10 s, so #a lasts to the end.
    let unwritten = resolve(vec![clip("a", "1", None, 4), clip("b", "after:a+0.5", Some("1"), 5)]);
    assert_eq!(codes(&unwritten), ["FS-W001"]);
    assert_eq!(unwritten.manifest.unwrap().clips[1].start, time("10.5"));
    let image = element("img", Some("a"), Some(0), 4, &[("data-fs-clip", ""), ("data-fs-start", "1")]);
    let image = resolve(vec![image, clip("b", "after:a-0.5", Some("1"), 5)]);
    assert_eq!(codes(&image), Vec::<&str>::new());
    assert_eq!(image.manifest.unwrap().clips[1].start, time("3.5"));
    // A nested composition, named with an offset.
    let nested = element(
        "div",
        Some("inner"),
        Some(0),
        4,
        &[("data-fs-composition", "inner"), ("data-fs-size", "10x10")],
    );
    let nested = resolve(vec![nested, clip("b", "after:inner+1", Some("1"), 5)]);
    assert_eq!(codes(&nested), ["FS-U006", "FS-U003"]);
    assert!(nested.diagnostics[1].description.contains("a nested composition"));
}

#[test]
fn s2_t011_a_fault_of_the_named_clip_is_reported_once_and_its_follower_is_not_reported_again() {
    let resolved = resolve(vec![clip("a", "1", Some("0"), 4), clip("b", "after:a-5", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E024"]);
}

#[test]
fn s2_t011_a_follower_that_lands_at_or_after_the_end_gets_the_warning_and_one_before_the_end_does_not() {
    // The composition lasts 10 s; #a ends at 9 s.
    let late = resolve(vec![clip("a", "8", Some("1"), 4), clip("b", "after:a+1", Some("1"), 5)]);
    assert_eq!(codes(&late), ["FS-W001"]);
    let ok = resolve(vec![clip("a", "8", Some("1"), 4), clip("b", "after:a+0.5", Some("1"), 5)]);
    assert_eq!(codes(&ok), Vec::<&str>::new());
}

#[test]
fn s2_t011_a_manifest_with_an_offset_carries_the_form_and_a_plain_one_is_unchanged() {
    let resolved = resolve(vec![clip("a", "1", Some("1"), 4), clip("b", "after:a+0.5", Some("1"), 5)]);
    let json = serde_json::to_string(&resolved.manifest.unwrap()).unwrap();
    assert!(json.contains("\"relative_start\":\"after:a+0.5\""), "{json}");
}
