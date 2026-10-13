//! Reading and checking a composition written in XML.

use poc::anim::Prop;
use poc::model::{Align, Direction, Inline, Kind};
use poc::xml;
use std::path::Path;

const TITLE_CARD: &str = include_str!("../compositions/title-card.xml");

fn parse(source: &str) -> Result<poc::model::Composition, Vec<xml::Problem>> {
    xml::parse(source, Path::new("."))
}

#[test]
fn the_title_card_is_read_with_its_elements_in_document_order() {
    let composition = parse(TITLE_CARD).expect("the title card is a valid composition");
    assert_eq!((composition.width, composition.height, composition.fps), (1280, 720, 30));
    assert_eq!(composition.background, [0x10, 0x18, 0x20, 255]);
    let elements = &composition.root.children;
    let ids: Vec<&str> = elements.iter().map(|element| element.id.as_str()).collect();
    assert_eq!(ids, ["title", "bar", "tag", "subtitle", "badge"]);
    assert!(matches!(elements[0].kind, Kind::Text));
    let words: Vec<&str> = elements[0]
        .inline
        .iter()
        .filter_map(|part| match part {
            Inline::Words(words) => Some(words.trim()),
            Inline::Child(_) => None,
        })
        .filter(|words| !words.is_empty())
        .collect();
    assert_eq!(words, ["Framesmith"]);
    assert!(!elements[1].tracks.is_empty(), "the bar has motion");
}

#[test]
fn every_problem_is_reported_with_its_line() {
    let source = "<composition width=\"1280\" height=\"720\" duration=\"2\">\n  <rect width=\"10\" height=\"10\" colour=\"#fff\"/>\n  <circle/>\n  <rect width=\"ten\" height=\"10\">\n    <tween at=\"0\" duration=\"1\" ease=\"bouncy\" to-x=\"5\"/>\n  </rect>\n</composition>";
    let problems = parse(source).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "<rect> has no attribute `colour`"),
            (3, "<circle> is not a known element"),
            (4, "`width=\"ten\"` is not a number or a percentage"),
            (5, "`ease=\"bouncy\"` is not a known easing"),
        ]
    );
}

#[test]
fn a_file_that_is_not_xml_is_one_problem() {
    let problems = parse("<composition").err().expect("the file is not XML");
    assert_eq!(problems.len(), 1);
    assert!(problems[0].message.starts_with("the file is not well-formed XML"));
}

#[test]
fn a_text_passes_its_properties_to_its_spans_and_a_span_may_change_them() {
    let source = r##"<composition width="64" height="32" duration="1">
  <text id="line" font-family="Arial" font-size="40" letter-spacing="-0.01em" color="#ffffff">one <span id="b" font-weight="800" color="#ff0000">two</span></text>
</composition>"##;
    let composition = parse(source).expect("the composition is valid");
    let line = &composition.root.children[0];
    let span = &line.children[0];
    assert!(matches!(span.kind, Kind::Span));
    assert_eq!(
        (span.text.family.as_str(), span.text.size, span.text.weight),
        ("Arial", 40.0, 800.0)
    );
    assert!(
        (span.text.letter_spacing + 0.4).abs() < 1e-6,
        "ems are taken of the size where they are written"
    );
    assert_eq!((line.text.color, span.text.color), ([255, 255, 255, 255], [255, 0, 0, 255]));
    assert!(matches!(line.inline[..], [Inline::Words(_), Inline::Child(0)]));
}

#[test]
fn a_row_or_column_and_the_motion_of_more_properties_are_read() {
    let source = r##"<composition width="64" height="32" duration="1">
  <box id="col" flex-direction="column" justify-content="center" align-items="flex-start" gap="6" padding="1 2 3 4">
    <box width="10" height="10" background="linear-gradient(90deg, #000000 0%, #ffffff80 100%)" transform-origin="0 50%">
      <tween at="0" duration="1" ease="out-back(2)" from-scale-x="0" from-blur="4" to-scale-x="1" to-blur="0"/>
    </box>
  </box>
</composition>"##;
    let composition = parse(source).expect("the composition is valid");
    let column = &composition.root.children[0];
    let flex = column.flow.flex.expect("the box is a column");
    assert_eq!(
        (flex.direction, flex.justify, flex.align, flex.gap),
        (Direction::Column, Align::Centre, Align::Start, 6.0)
    );
    assert_eq!(column.flow.padding, [1.0, 2.0, 3.0, 4.0]);
    let moved: Vec<Prop> = column.children[0].tracks.iter().map(|track| track.prop).collect();
    assert_eq!(moved, [Prop::ScaleX, Prop::Blur]);
}

#[test]
fn a_sound_is_read_with_its_volume_points() {
    let source = r##"<composition width="64" height="32" duration="10">
  <audio id="bed" src="bed.mp3" start="1" duration="8" media-start="0.5" volume="0.8">
    <volume at="0" value="0.2"/>
    <volume at="4" value="0.6"/>
  </audio>
</composition>"##;
    let composition = parse(source).expect("the composition is valid");
    let bed = &composition.audio[0];
    assert_eq!((bed.start, bed.duration, bed.media_start, bed.volume), (1.0, 8.0, 0.5, 0.8));
    assert_eq!(bed.automation, [(0.0, 0.2), (4.0, 0.6)]);
}

#[test]
fn a_wrong_value_names_what_is_allowed() {
    let source = "<composition width=\"64\" height=\"32\" duration=\"1\">\n  <box mix-blend-mode=\"plus-lighter\" background=\"red\"/>\n  <text><box/></text>\n</composition>";
    let problems = parse(source).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "`background=\"red\"` is not a colour or a list of gradients"),
            (
                2,
                "`mix-blend-mode=\"plus-lighter\"` is not one of: normal, multiply, screen, overlay, darken, lighten, color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue, saturation, color, luminosity"
            ),
            (3, "<box> is not known inside a text; only <span> is"),
        ]
    );
}

#[test]
fn background_layers_are_read_with_their_size_place_and_repeat() {
    use poc::model::Dim;
    use poc::style::{Along, Paint, Reach};
    let source = r##"<composition width="64" height="32" duration="1">
  <box background="radial-gradient(circle, #ffffff29 1.4, #00000000 2), linear-gradient(#5a5c5e 0%, #5a5c5e 100%), #111314"
       background-size="24 24, 2 100%, auto" background-position="0 0, 50% 0, 0 0" background-repeat="repeat, no-repeat, repeat"/>
  <box background="#ff0000, #00ff00" background-size="8 8"/>
</composition>"##;
    let composition = parse(source).expect("the composition is valid");
    let layers = &composition.root.children[0].look.background;
    assert_eq!(layers.len(), 3);
    let Paint::Radial { reach, centre, stops } = &layers[0].paint else {
        panic!("the first layer is a radial gradient")
    };
    assert_eq!((*reach, *centre), (Reach::CircleToCorner, (0.5, 0.5)));
    assert_eq!((stops[0].0, stops[1].0), (Along::Px(1.4), Along::Px(2.0)));
    assert_eq!(layers[0].size, Some((Dim::Px(24.0), Dim::Px(24.0))));
    assert_eq!(layers[1].size, Some((Dim::Px(2.0), Dim::Share(1.0))));
    assert_eq!(layers[1].position, (Dim::Share(0.5), Dim::Px(0.0)));
    assert_eq!((layers[0].repeat, layers[1].repeat, layers[2].repeat), (true, false, true));
    assert_eq!(layers[2].size, None);
    // One entry applies to every layer.
    let both = &composition.root.children[1].look.background;
    assert_eq!(
        (both[0].size, both[1].size),
        (Some((Dim::Px(8.0), Dim::Px(8.0))), Some((Dim::Px(8.0), Dim::Px(8.0))))
    );
    // A tile that is the box itself is no tile; any other is.
    assert_eq!(layers[2].tile(64.0, 32.0), None);
    let bar = layers[1].tile(64.0, 32.0).expect("the bar is a tile");
    assert_eq!(
        (bar.rect.x0, bar.rect.y0, bar.rect.x1, bar.rect.y1, bar.repeat),
        (31.0, 0.0, 33.0, 32.0, false)
    );
}

#[test]
fn a_layer_placement_that_does_not_fit_the_layers_is_a_problem() {
    let source = "<composition width=\"64\" height=\"32\" duration=\"1\">\n  <box background=\"#ff0000, #00ff00, #0000ff\" background-size=\"8 8, 4 4\"/>\n  <box background-repeat=\"no-repeat\"/>\n  <box background=\"#ff0000\" background-size=\"8\" background-repeat=\"round\"/>\n</composition>";
    let problems = parse(source).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (
                2,
                "`background-size` has 2 entries for 3 layers; it takes one for each layer, or one for all"
            ),
            (3, "`background-repeat` needs a `background` to place"),
            (4, "`background-size=\"8\"` is not a list of `auto` or a width and a height"),
            (4, "`background-repeat=\"round\"` is not a list of `repeat` or `no-repeat`"),
        ]
    );
}
