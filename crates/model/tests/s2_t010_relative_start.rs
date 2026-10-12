//! Sprint 2, ticket T010, at the level of the resolver: how a start that begins `after:` is read, how the
//! named clip is found, and the start that results. The page and the host resolve with this code.

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

fn composition() -> RawElement {
    element(
        "div",
        None,
        None,
        3,
        &[
            ("data-fs-composition", "main"),
            ("data-fs-size", "64x36"),
            ("data-fs-duration", "10"),
        ],
    )
}

/// A clip with an id, a start, and a duration when one is given; its line is its place in the list.
fn clip(id: &str, start: &str, duration: Option<&str>, line: u32) -> RawElement {
    let mut attrs = vec![("data-fs-clip", ""), ("data-fs-start", start)];
    if let Some(duration) = duration {
        attrs.push(("data-fs-duration", duration));
    }
    element("div", Some(id), Some(0), line, &attrs)
}

fn resolve(clips: Vec<RawElement>) -> Resolved {
    let mut elements = vec![composition()];
    elements.extend(clips);
    let raw = RawDoc { elements, motion: vec![] };
    Draft::read(&raw, "index.html").finish(&mut |_| Ok(vec![]))
}

fn codes(resolved: &Resolved) -> Vec<&str> {
    resolved.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn time(text: &str) -> Time {
    Time::parse(text).unwrap()
}

#[test]
fn the_start_of_a_follower_is_the_start_plus_the_duration_of_the_named_clip_exactly() {
    // 1.1 + 2.2 is exactly 3.3, not the binary number nearest to it.
    let resolved = resolve(vec![clip("a", "1.1", Some("2.2"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].start, time("3.3"));
    assert_eq!(manifest.clips[1].relative_start.as_deref(), Some("after:a"));
    assert_eq!(manifest.clips[0].relative_start, None);
}

#[test]
fn the_named_clip_may_come_later_in_the_file() {
    let resolved = resolve(vec![clip("b", "after:a", Some("1"), 4), clip("a", "2", Some("0.5"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(resolved.manifest.unwrap().clips[0].start, time("2.5"));
}

#[test]
fn spaces_after_the_colon_and_at_the_ends_are_ignored_and_the_form_is_kept_as_written() {
    let resolved = resolve(vec![clip("a", "0", Some("1"), 4), clip("b", "  after:  a  ", Some("1"), 5)]);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!(manifest.clips[1].start, time("1"));
    assert_eq!(manifest.clips[1].relative_start.as_deref(), Some("after:  a"));
}

/// A clip `a` and a follower written `written`, with the extra clips given (all absolute, one second long).
fn follower(written: &str, ids: &[&str]) -> Resolved {
    let mut clips = vec![];
    for (i, id) in ids.iter().enumerate() {
        clips.push(clip(id, "0", Some("1"), 4 + i as u32));
    }
    clips.push(clip("b", written, Some("1"), 20));
    resolve(clips)
}

#[test]
fn step_one_a_text_that_is_exactly_the_id_of_a_clip_is_the_plain_form_whatever_characters_it_holds() {
    for id in [
        "lower-third",
        "scene-2",
        "intro-0.5",
        "a.b",
        "x_y",
        "Intro",
        "a+b",
        "a -1",
        "-1",
        "+1",
        "a b",
    ] {
        let resolved = follower(&format!("after:{id}"), &[id]);
        assert_eq!(codes(&resolved), Vec::<&str>::new(), "{id}");
        let manifest = resolved.manifest.unwrap();
        assert_eq!(manifest.clips[1].start, time("1"), "{id}");
        assert_eq!(manifest.clips[1].relative_start, Some(format!("after:{id}")), "{id}");
    }
}

#[test]
fn step_three_any_other_text_is_the_id_of_no_clip() {
    for (written, id) in [
        ("after:lower-third", "lower-third"),
        ("after:scene-two", "scene-two"),
        ("after:a-b", "a-b"),
        ("after:a-1x", "a-1x"),
        ("after:a-", "a-"),
        ("after:a b", "a b"),
        ("after:+1", "+1"),
        ("after:-1", "-1"),
        ("after:A", "A"),
    ] {
        let resolved = follower(written, &["a"]);
        assert_eq!(codes(&resolved), ["FS-E050"], "{written}");
        assert!(
            resolved.diagnostics[0]
                .description
                .contains(&format!("no clip with the id \"{id}\"")),
            "{written}: {}",
            resolved.diagnostics[0].description
        );
        // The remedy no longer talks about a hyphen.
        let remedy = &resolved.diagnostics[0].remedy;
        assert!(!remedy.contains("joined"), "{written}: {remedy}");
    }
}

#[test]
fn the_same_text_is_the_plain_form_when_the_clip_is_there_and_an_offset_when_it_is_not() {
    for (written, id) in [
        ("after:first-0.5", "first-0.5"),
        ("after:scene-2", "scene-2"),
        ("after:lower-third", "lower-third"),
    ] {
        let present = follower(written, &[id]);
        assert_eq!(codes(&present), Vec::<&str>::new(), "{written} present");
        assert_eq!(present.manifest.unwrap().clips[1].start, time("1"));
        let absent = follower(written, &[]);
        // Ticket T011 built the offset: with no clip of that name the text is read both ways and is the
        // id of no clip (FS-E050), where T010 reported an offset that waits (FS-U003).
        assert_eq!(codes(&absent), ["FS-E050"], "{written} absent");
    }
}

#[test]
fn a_clip_that_names_itself_by_a_text_with_a_hyphen_is_still_a_clip_that_names_itself() {
    let resolved = resolve(vec![clip("lower-third", "after:lower-third", Some("1"), 4)]);
    assert_eq!(codes(&resolved), ["FS-E049"]);
    // With the sign and a number it is an offset from the clip `lower`, which is the clip itself (T011).
    let resolved = resolve(vec![clip("lower", "after:lower-3", Some("1"), 4)]);
    assert_eq!(codes(&resolved), ["FS-E049"]);
}

#[test]
fn a_start_that_is_neither_a_time_nor_a_relative_start_is_not_a_time() {
    for written in ["after:", "after:   ", "After:a", "afterwards:a"] {
        let resolved = resolve(vec![clip("a", "0", Some("1"), 4), clip("b", written, Some("1"), 5)]);
        assert_eq!(codes(&resolved), ["FS-E021"], "{written}");
    }
}

#[test]
fn a_missing_clip_and_a_clip_that_names_itself_have_codes_of_their_own() {
    let missing = resolve(vec![clip("b", "after:nothing", Some("1"), 4)]);
    assert_eq!(codes(&missing), ["FS-E050"]);
    assert_eq!(missing.diagnostics[0].line, 4);
    let itself = resolve(vec![clip("b", "after:b", Some("1"), 4)]);
    assert_eq!(codes(&itself), ["FS-E049"]);
    assert_ne!(missing.diagnostics[0].code, itself.diagnostics[0].code);
}

#[test]
fn a_chain_is_resolved_a_circle_is_an_error_and_a_named_clip_with_no_written_duration_is_resolved_too() {
    // (The offset form no longer waits: ticket T011. A chain no longer waits either: ticket T012; a circle
    // is the error FS-E053. A named clip with no written duration no longer waits: ticket T013.)
    let chain = resolve(vec![
        clip("a", "1", Some("1"), 4),
        clip("b", "after:a", Some("1"), 5),
        clip("c", "after:b", Some("1"), 6),
    ]);
    assert_eq!(codes(&chain), Vec::<&str>::new());
    assert!(chain.manifest.is_some());
    let cycle = resolve(vec![clip("a", "after:b", Some("1"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&cycle), ["FS-E053"]);
    assert!(cycle.manifest.is_none());
    // The composition lasts 10 s, so #a lasts to the end and #b starts there, at 10 s: it never shows.
    let unwritten = resolve(vec![clip("a", "1", None, 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&unwritten), ["FS-W001"]);
    assert_eq!(unwritten.manifest.unwrap().clips[1].start, time("10"));
    // An image with none lasts 3 s, so #b starts at 4 s.
    let image = element("img", Some("a"), Some(0), 4, &[("data-fs-clip", ""), ("data-fs-start", "1")]);
    let image = resolve(vec![image, clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&image), Vec::<&str>::new());
    assert_eq!(image.manifest.unwrap().clips[1].start, time("4"));
}

#[test]
fn a_nested_composition_that_is_named_is_refused_not_reported_as_missing() {
    let nested = element(
        "div",
        Some("inner"),
        Some(0),
        4,
        &[("data-fs-composition", "inner"), ("data-fs-size", "10x10")],
    );
    let resolved = resolve(vec![nested, clip("b", "after:inner", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-U006", "FS-U003"]);
}

#[test]
fn the_fault_of_the_named_clip_is_reported_once_and_its_follower_is_not_reported_again() {
    let resolved = resolve(vec![clip("a", "1", Some("0"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E024"]);
    let resolved = resolve(vec![clip("a", "-1", Some("1"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-E022"]);
}

#[test]
fn a_follower_that_starts_at_or_after_the_end_of_the_composition_gets_the_warning_for_it() {
    // The composition lasts 10 s; #a ends at 10 s, so #b starts at the end and never shows.
    let resolved = resolve(vec![clip("a", "8", Some("2"), 4), clip("b", "after:a", Some("1"), 5)]);
    assert_eq!(codes(&resolved), ["FS-W001"]);
    assert!(resolved.manifest.is_some());
}

#[test]
fn a_manifest_of_absolute_starts_does_not_carry_the_new_field() {
    let resolved = resolve(vec![clip("a", "1", Some("1"), 4)]);
    let json = serde_json::to_string(&resolved.manifest.unwrap()).unwrap();
    assert!(!json.contains("relative_start"), "{json}");
}
