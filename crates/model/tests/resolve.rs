use model::*;
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

fn composition(attrs: &[(&str, &str)]) -> RawElement {
    let mut all = vec![("data-fs-composition", "main"), ("data-fs-size", "1920x1080")];
    all.extend_from_slice(attrs);
    element("div", None, None, 3, &all)
}

fn resolve(raw: &RawDoc) -> Resolved {
    Draft::read(raw, "index.html").finish(&mut |selector| match selector {
        "#none" => Ok(vec![]),
        "a:hover" => Err(SelectorProblem::Unsupported),
        "#" => Err(SelectorProblem::Invalid),
        ".three" => Ok(vec![1, 2, 3]),
        _ => Ok(vec![7]),
    })
}

fn codes(resolved: &Resolved) -> Vec<&str> {
    resolved.diagnostics.iter().map(|d| d.code.as_str()).collect()
}

fn secs(text: &str) -> Time {
    Time::parse(text).unwrap()
}

fn motion(text: &str) -> Vec<RawMotion> {
    vec![RawMotion {
        text: text.to_string(),
        line: 20,
    }]
}

#[test]
fn t007_c1_written_facts_are_read_back() {
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-fps", "30"), ("data-fs-duration", "6")])],
        motion: vec![],
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    let manifest = resolved.manifest.unwrap();
    assert_eq!((manifest.name.as_str(), manifest.width, manifest.height), ("main", 1920, 1080));
    assert_eq!((manifest.fps, manifest.fps_written), (30, true));
    assert_eq!((manifest.length, manifest.length_written), (secs("6"), true));
    assert_eq!(timebase::frame_count(manifest.length, manifest.fps), 180);
}

#[test]
fn t008_c1_a_missing_rate_is_thirty_and_marked() {
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "2")])],
        motion: vec![],
    };
    let manifest = resolve(&raw).manifest.unwrap();
    assert_eq!((manifest.fps, manifest.fps_written), (30, false));
}

#[test]
fn t008_c4_a_malformed_rate_is_an_error() {
    for bad in ["29.97", "0", "fast", "241", "-5"] {
        let raw = RawDoc {
            elements: vec![composition(&[("data-fs-fps", bad), ("data-fs-duration", "2")])],
            motion: vec![],
        };
        let resolved = resolve(&raw);
        assert_eq!(codes(&resolved), vec!["FS-E008"], "{bad}");
        assert_eq!(resolved.diagnostics[0].line, 3);
        assert!(resolved.manifest.is_none());
    }
}

#[test]
fn t009_declaration_errors() {
    let none = resolve(&RawDoc::default());
    assert_eq!(codes(&none), vec!["FS-E003"]);

    let case = |attrs: &[(&str, &str)]| {
        let raw = RawDoc {
            elements: vec![element("div", None, None, 5, attrs)],
            motion: vec![],
        };
        resolve(&raw)
    };
    let no_size = case(&[("data-fs-composition", "main"), ("data-fs-duration", "1")]);
    assert_eq!(codes(&no_size), vec!["FS-E005"]);
    for bad in ["1920", "1920x", "wide x tall", "1920x1080x3", "0x0", "99999x10"] {
        let r = case(&[("data-fs-composition", "main"), ("data-fs-size", bad), ("data-fs-duration", "1")]);
        assert_eq!(codes(&r), vec!["FS-E006"], "{bad}");
    }
    for odd in ["1921x1080", "1920x1081"] {
        let r = case(&[("data-fs-composition", "main"), ("data-fs-size", odd), ("data-fs-duration", "1")]);
        assert_eq!(codes(&r), vec!["FS-E007"], "{odd}");
        assert!(r.diagnostics[0].description.contains("both be even"));
    }
    for (bad, code) in [("0", "FS-E010"), ("-2", "FS-E010"), ("long", "FS-E009"), ("1e3", "FS-E009")] {
        let r = case(&[
            ("data-fs-composition", "main"),
            ("data-fs-size", "640x360"),
            ("data-fs-duration", bad),
        ]);
        assert_eq!(codes(&r), vec![code], "{bad}");
        assert_eq!(r.diagnostics[0].line, 5);
    }
    // Three separate errors are all listed in one pass.
    let three = case(&[
        ("data-fs-composition", "my comp"),
        ("data-fs-size", "641x360"),
        ("data-fs-duration", "0"),
    ]);
    assert_eq!(codes(&three), vec!["FS-E004", "FS-E007", "FS-E010"]);
    assert!(three.diagnostics.iter().all(|d| d.kind == Severity::Error && !d.remedy.is_empty()));
}

#[test]
fn t012_clip_errors_and_the_warning() {
    let raw = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "4")]),
            element("p", Some("a"), Some(0), 5, &[("data-fs-clip", ""), ("data-fs-duration", "1")]),
            element(
                "p",
                Some("b"),
                Some(0),
                6,
                &[("data-fs-clip", ""), ("data-fs-start", "-1"), ("data-fs-duration", "1")],
            ),
            element(
                "p",
                Some("c"),
                Some(0),
                7,
                &[("data-fs-clip", ""), ("data-fs-start", "1"), ("data-fs-duration", "0")],
            ),
            element(
                "p",
                Some("d"),
                Some(0),
                8,
                &[("data-fs-clip", ""), ("data-fs-start", "1"), ("data-fs-duration", "-3")],
            ),
            element(
                "p",
                Some("e"),
                Some(0),
                9,
                &[("data-fs-clip", ""), ("data-fs-start", "soon"), ("data-fs-duration", "x")],
            ),
        ],
        motion: vec![],
    };
    let resolved = resolve(&raw);
    assert_eq!(
        codes(&resolved),
        vec!["FS-E020", "FS-E022", "FS-E024", "FS-E024", "FS-E021", "FS-E023"]
    );
    assert_eq!(resolved.diagnostics[0].line, 5);
    assert!(resolved.manifest.is_none());

    let late = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "4")]),
            element(
                "p",
                Some("late"),
                Some(0),
                5,
                &[("data-fs-clip", ""), ("data-fs-start", "4"), ("data-fs-duration", "1")],
            ),
            element(
                "p",
                None,
                Some(0),
                6,
                &[("data-fs-clip", ""), ("data-fs-start", "9"), ("data-fs-duration", "1")],
            ),
            element(
                "p",
                None,
                Some(0),
                7,
                &[("data-fs-clip", ""), ("data-fs-start", "3.9"), ("data-fs-duration", "1")],
            ),
        ],
        motion: vec![],
    };
    let resolved = resolve(&late);
    assert_eq!(codes(&resolved), vec!["FS-W001", "FS-W001"]);
    assert!(resolved.diagnostics[0].description.contains("#late"));
    assert!(resolved.diagnostics[1].description.contains("number 2 <p>"));
    assert_eq!(resolved.manifest.unwrap().clips.len(), 3);
    assert_eq!(model::exit_code(&resolved.diagnostics), 0);
}

#[test]
fn t011_c4_nested_clips_know_their_enclosing_clip() {
    let raw = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "4")]),
            element(
                "div",
                Some("outer"),
                Some(0),
                5,
                &[
                    ("data-fs-clip", ""),
                    ("data-fs-start", "1"),
                    ("data-fs-duration", "2"),
                    ("data-fs-track", "2"),
                ],
            ),
            element(
                "p",
                Some("inner"),
                Some(1),
                6,
                &[("data-fs-clip", ""), ("data-fs-start", "0"), ("data-fs-duration", "4")],
            ),
            element(
                "p",
                Some("other"),
                Some(0),
                7,
                &[("data-fs-clip", ""), ("data-fs-start", "0"), ("data-fs-duration", "1")],
            ),
        ],
        motion: vec![],
    };
    let manifest = resolve(&raw).manifest.unwrap();
    let parents: Vec<Option<usize>> = manifest.clips.iter().map(|c| c.parent).collect();
    assert_eq!(parents, vec![None, Some(0), None]);
    assert_eq!(manifest.clips[0].track, Some(2));
    assert_eq!(manifest.clips[0].end(), secs("3"));
}

#[test]
fn t021_durations_are_worked_out() {
    let tweens = r##"[{ "target": "#t", "to": { "x": 1 }, "at": 4, "duration": 1 }]"##;
    let clip = |tag: &str, id: &str, line: u32, start: &str| {
        element(tag, Some(id), Some(0), line, &[("data-fs-clip", ""), ("data-fs-start", start)])
    };
    let written = element(
        "p",
        Some("w"),
        Some(0),
        7,
        &[("data-fs-clip", ""), ("data-fs-start", "0"), ("data-fs-duration", "2")],
    );
    let facts = |raw: &RawDoc| -> Vec<(Time, bool)> {
        let manifest = resolve(raw).manifest.unwrap();
        manifest.clips.iter().map(|c| (c.duration, c.duration_written)).collect()
    };

    // An image clip lasts 3 seconds; any other clip lasts until the end of the composition; a written
    // duration is kept.
    let raw = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "8")]),
            clip("img", "pic", 5, "1"),
            clip("p", "t", 6, "1"),
            written.clone(),
        ],
        motion: motion(tweens),
    };
    assert_eq!(facts(&raw), vec![(secs("3"), false), (secs("7"), false), (secs("2"), true)]);
    // The same with the length worked out: the motion ends at 5 seconds.
    let raw = RawDoc {
        elements: vec![
            composition(&[]),
            clip("img", "pic", 5, "1"),
            clip("p", "t", 6, "1"),
            written.clone(),
        ],
        motion: motion(tweens),
    };
    assert_eq!(facts(&raw), vec![(secs("3"), false), (secs("4"), false), (secs("2"), true)]);
    // A written length is enough: no motion is needed.
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "8")]), clip("p", "t", 6, "0")],
        motion: vec![],
    };
    assert_eq!(facts(&raw), vec![(secs("8"), false)]);
    // With no motion and no written length, a clip that has its own end gives the length.
    let raw = RawDoc {
        elements: vec![composition(&[]), clip("p", "t", 6, "0"), written.clone()],
        motion: vec![],
    };
    assert_eq!(facts(&raw), vec![(secs("2"), false), (secs("2"), true)]);

    // A clip that starts at or after the end never shows: no time is left for it.
    let raw = RawDoc {
        elements: vec![composition(&[]), clip("p", "t", 6, "6")],
        motion: motion(tweens),
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), vec!["FS-W001"]);
    assert_eq!(resolved.manifest.unwrap().clips[0].duration, Time::ZERO);

    // No written length, no motion and no clip with its own end: nothing to last until.
    let nothing = RawDoc {
        elements: vec![composition(&[]), clip("p", "t", 6, "0")],
        motion: vec![],
    };
    let resolved = resolve(&nothing);
    assert_eq!(codes(&resolved), vec!["FS-E025"]);
    assert!(resolved.diagnostics[0].description.contains("#t"));
    assert!(resolved.manifest.is_none());
}

#[test]
fn t022_length_is_worked_out() {
    let tweens = r##"[{ "target": "#c", "to": { "x": 1 }, "at": 4, "duration": 1 }]"##;
    let clip_until = |end: &str| {
        element(
            "p",
            Some("c"),
            Some(0),
            5,
            &[("data-fs-clip", ""), ("data-fs-start", "0"), ("data-fs-duration", end)],
        )
    };
    let image_from_4 = || element("img", None, Some(0), 5, &[("data-fs-clip", ""), ("data-fs-start", "4")]);
    let length = |elements: Vec<RawElement>, tweens: &str| {
        let raw = RawDoc {
            elements,
            motion: if tweens.is_empty() { vec![] } else { motion(tweens) },
        };
        let manifest = resolve(&raw).manifest.unwrap();
        (manifest.length, manifest.length_written)
    };

    // With motion, the end of the motion is the length, whether the last clip ends before it ...
    assert_eq!(length(vec![composition(&[]), clip_until("4")], tweens), (secs("5"), false));
    assert_eq!(timebase::frame_count(secs("5"), 30), 150);
    // ... or after it, written or worked out.
    assert_eq!(length(vec![composition(&[]), clip_until("7")], tweens), (secs("5"), false));
    assert_eq!(
        length(vec![composition(&[]), clip_until("1"), image_from_4()], tweens),
        (secs("5"), false)
    );
    // A set tween ends at its own time.
    let set_at = |at: &str| format!(r##"[{{ "target": "#c", "set": {{ "x": 1 }}, "at": {at} }}]"##);
    assert_eq!(length(vec![composition(&[]), clip_until("7")], &set_at("2")), (secs("2"), false));
    // A written length wins.
    assert_eq!(
        length(vec![composition(&[("data-fs-duration", "2")]), clip_until("7")], tweens),
        (secs("2"), true)
    );
    // With no motion, the last clip that has its own end gives the length: a written one, or an image
    // clip, which lasts 3 seconds (from 4 s until 7 s).
    assert_eq!(length(vec![composition(&[]), clip_until("4")], ""), (secs("4"), false));
    assert_eq!(
        length(vec![composition(&[]), clip_until("4"), image_from_4()], ""),
        (secs("7"), false)
    );
    // Motion that ends at 0 counts as no motion.
    assert_eq!(length(vec![composition(&[]), clip_until("4")], &set_at("0")), (secs("4"), false));
    // Nothing to work it out from.
    let raw = RawDoc {
        elements: vec![composition(&[])],
        motion: vec![],
    };
    assert_eq!(codes(&resolve(&raw)), vec!["FS-E011"]);
}

#[test]
fn t020_motion_problems_carry_absolute_lines() {
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "4")])],
        motion: motion("[\n { \"target\": \"#none\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 },\n { \"target\": \"#ok\", \"to\": { \"x\": 1 }, \"at\": -1, \"duration\": 1 },\n { \"target\": \"a:hover\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 },\n { \"target\": \"#\", \"to\": { \"x\": 1 }, \"at\": 0, \"duration\": 1 }\n]"),
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), vec!["FS-E044", "FS-E043", "FS-U020", "FS-E048"]);
    let lines: Vec<u32> = resolved.diagnostics.iter().map(|d| d.line).collect();
    assert_eq!(lines, vec![21, 22, 23, 24]);
    assert!(resolved.diagnostics[0].description.contains("#none"));
}

#[test]
fn tween_facts_record_how_many_elements_match() {
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "4")])],
        motion: motion(r##"{ "composition": "main", "tweens": [{ "target": ".three", "to": { "x": 1 }, "at": 0, "duration": 1 }] }"##),
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), Vec::<&str>::new());
    assert_eq!(resolved.manifest.unwrap().tweens[0].matches, 3);
    assert_eq!(resolved.targets, vec![vec![1, 2, 3]]);

    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "4")])],
        motion: motion(r##"{ "composition": "other", "tweens": [] }"##),
    };
    assert_eq!(codes(&resolve(&raw)), vec!["FS-E046"]);
}

#[test]
fn t029_deferred_timing_and_structure_are_unsupported() {
    let raw = RawDoc {
        elements: vec![
            composition(&[
                ("data-fs-duration", "4"),
                ("data-fs-fps", "30000/1001"),
                ("data-fs-var-title", "Hello"),
            ]),
            element(
                "p",
                Some("a"),
                Some(0),
                5,
                &[("data-fs-clip", ""), ("data-fs-start", "after:intro"), ("data-fs-duration", "1")],
            ),
            element(
                "div",
                None,
                Some(0),
                6,
                &[("data-fs-composition", "inner"), ("data-fs-size", "10x10")],
            ),
            element(
                "div",
                None,
                None,
                9,
                &[("data-fs-composition", "second"), ("data-fs-size", "10x10")],
            ),
        ],
        motion: vec![],
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), vec!["FS-U004", "FS-U007", "FS-U003", "FS-U006", "FS-U005"]);
    assert_eq!(model::exit_code(&resolved.diagnostics), 2);
    assert!(resolved.diagnostics.iter().all(|d| d.kind == Severity::Unsupported));
}

#[test]
fn t032_c1_unknown_attributes_are_never_ignored() {
    let raw = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "4"), ("data-fs-fpss", "30")]),
            element("p", None, Some(0), 5, &[("data-fs-clipp", "")]),
        ],
        motion: vec![],
    };
    let resolved = resolve(&raw);
    assert_eq!(codes(&resolved), vec!["FS-X001", "FS-X001"]);
    assert!(resolved.diagnostics[0].description.contains("data-fs-fpss"));
    assert_eq!(resolved.diagnostics[1].line, 5);
    assert_eq!(model::exit_code(&resolved.diagnostics), 2);
}

#[test]
fn misplaced_attributes_are_errors() {
    let raw = RawDoc {
        elements: vec![
            composition(&[("data-fs-duration", "4")]),
            element("p", None, Some(0), 5, &[("data-fs-start", "1")]),
            element(
                "p",
                None,
                None,
                9,
                &[("data-fs-clip", ""), ("data-fs-start", "1"), ("data-fs-duration", "1")],
            ),
        ],
        motion: vec![],
    };
    assert_eq!(codes(&resolve(&raw)), vec!["FS-E026", "FS-E027"]);
}

#[test]
fn t028_c4_errors_outrank_unsupported_uses() {
    let raw = RawDoc {
        elements: vec![composition(&[("data-fs-duration", "0"), ("data-fs-var-x", "1")])],
        motion: vec![],
    };
    let resolved = resolve(&raw);
    assert_eq!(model::exit_code(&resolved.diagnostics), 1);
}
