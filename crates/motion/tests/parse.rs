use motion::*;
use timebase::Time;

fn codes(block: &Block) -> Vec<&'static str> {
    block.findings.iter().map(|f| f.code).collect()
}

#[test]
fn reads_the_four_kinds() {
    let block = parse_block(
        r##"{ "composition": "main", "tweens": [
          { "target": "#a", "to": { "x": 300 }, "at": 1, "duration": 1 },
          { "target": "#a", "from": { "y": 60, "opacity": 0 }, "at": 1, "duration": 0.5, "ease": "out-cubic" },
          { "target": ".b", "from": { "x": 0 }, "to": { "x": 10 }, "at": 0, "duration": 2, "kind": "from-to" },
          { "target": ".b", "set": { "scale": 2 }, "at": 2 }
        ] }"##,
    );
    assert_eq!(block.findings, vec![]);
    assert_eq!(block.written, 4);
    let kinds: Vec<Kind> = block.tweens.iter().map(|(t, _)| t.kind).collect();
    assert_eq!(kinds, vec![Kind::To, Kind::From, Kind::FromTo, Kind::Set]);
    assert_eq!(block.tweens[1].0.ease, Ease::OutCubic);
    assert_eq!(block.tweens[1].0.from[Prop::Opacity.index()], Some(0.0));
    assert_eq!(block.tweens[1].1, 3);
    assert_eq!(block.tweens[3].0.duration, Time::ZERO);
    assert_eq!(block.composition, Some(("main".to_string(), 1)));
}

#[test]
fn a_bare_list_is_a_block() {
    let block = parse_block(r##"[ { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1 } ]"##);
    assert_eq!(block.findings, vec![]);
    assert_eq!(block.tweens.len(), 1);
}

#[test]
fn t020_c2_invalid_json_names_the_line() {
    let block = parse_block("{ \"tweens\": [\n  { \"target\": \"#a\" \"to\": {} }\n] }");
    assert_eq!(codes(&block), vec!["FS-E040"]);
    assert_eq!(block.findings[0].line, 2);
}

#[test]
fn t020_c3_missing_fields_are_named() {
    let block = parse_block(
        r##"[ { "to": { "x": 1 }, "at": 0, "duration": 1 },
             { "target": "#a", "at": 0, "duration": 1 },
             { "target": "#b", "to": { "x": 1 }, "duration": 1 } ]"##,
    );
    assert_eq!(codes(&block), vec!["FS-E042", "FS-E042", "FS-E042"]);
    assert!(block.findings[0].description.contains("Tween 1") && block.findings[0].description.contains("target"));
    assert!(block.findings[1].description.contains("Tween 2") && block.findings[1].description.contains("no values"));
    assert!(block.findings[2].description.contains("#b") && block.findings[2].description.contains("\"at\""));
    assert!(block.tweens.is_empty());
}

#[test]
fn t020_c4_negative_times_are_errors() {
    let block = parse_block(
        r##"[ { "target": "#a", "to": { "x": 1 }, "at": -1, "duration": 1 },
             { "target": "#b", "to": { "x": 1 }, "at": 0, "duration": -0.5 } ]"##,
    );
    assert_eq!(codes(&block), vec!["FS-E043", "FS-E043"]);
    assert!(block.findings[0].description.contains("#a"));
    assert!(block.findings[1].description.contains("#b"));
}

#[test]
fn t031_recognised_but_unbuilt_names_are_unsupported() {
    let block = parse_block(
        r##"[ { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "ease": "out-bounce" },
             { "target": "#a", "to": { "color": 1 }, "at": 0, "duration": 1 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "repeat": 2 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "stagger": 0.1 },
             { "target": "#a", "keyframes": [], "at": 0, "duration": 1 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "ease": "steps(4)" } ]"##,
    );
    assert_eq!(
        codes(&block),
        vec!["FS-U011", "FS-U012", "FS-U013", "FS-U013", "FS-U010", "FS-E042", "FS-U011"]
    );
    assert!(block.tweens.is_empty());
}

#[test]
fn t032_unrecognised_names_are_unknown() {
    let block = parse_block(
        r##"{ "speed": 2, "tweens": [
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "durration": 2 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "kind": "slide" },
             { "target": "#a", "to": { "left": 1 }, "at": 0, "duration": 1 },
             { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "ease": "swoosh" } ] }"##,
    );
    assert_eq!(codes(&block), vec!["FS-X002", "FS-X002", "FS-X003", "FS-X004", "FS-X005"]);
}

#[test]
fn kind_must_agree_with_the_values() {
    let block = parse_block(
        r##"[ { "target": "#a", "to": { "x": 1 }, "at": 0, "duration": 1, "kind": "from" },
             { "target": "#a", "set": { "x": 1 }, "at": 0, "duration": 1 },
             { "target": "#a", "set": { "x": 1 }, "to": { "x": 2 }, "at": 0 },
             { "target": "#a", "from": { "x": 1 }, "to": { "y": 2 }, "at": 0, "duration": 1 } ]"##,
    );
    assert_eq!(codes(&block), vec!["FS-E047", "FS-E047", "FS-E047", "FS-E047"]);
}

#[test]
fn values_must_be_numbers_in_range() {
    let block = parse_block(
        r##"[ { "target": "#a", "to": { "x": "300px" }, "at": 0, "duration": 1 },
             { "target": "#a", "to": { "opacity": 1.5 }, "at": 0, "duration": 1 },
             { "target": "#a", "to": { "x": 1 }, "at": 1e2, "duration": 1 },
             { "target": "#a", "to": {}, "at": 0, "duration": 1 } ]"##,
    );
    assert_eq!(codes(&block), vec!["FS-E045", "FS-E045", "FS-E045", "FS-E042"]);
}
