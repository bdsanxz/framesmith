//! Shared styles: attributes written once in a `<style>` and taken by elements with `class`.

use poc::model::{Composition, Dim, Node};
use poc::style::Paint;
use poc::xml;
use std::path::Path;

fn parse(source: &str) -> Result<Composition, Vec<xml::Problem>> {
    xml::parse(source, Path::new("."))
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/styles")
}

#[test]
fn an_element_takes_the_attributes_of_the_styles_it_names() {
    let composition = parse(
        r##"<composition width="640" height="360" fps="30" duration="1">
  <style id="label" font-size="27" letter-spacing="2.5" color="#696b6d"/>
  <style id="loud" color="#ffffff" font-weight="700"/>
  <text id="plain" class="label" left="0" top="0">TITLE</text>
  <text id="both" class="label loud" left="0" top="40">CHAPTER</text>
  <text id="turned" class="loud label" left="0" top="80">CHAPTER</text>
  <text id="own" class="label loud" left="0" top="120" color="#ff4040" font-size="40">LIVE</text>
  <text id="none" left="0" top="160">BARE</text>
</composition>"##,
    )
    .expect("the composition is valid");
    let elements = &composition.root.children;
    let ids: Vec<&str> = elements.iter().map(|element| element.id.as_str()).collect();
    assert_eq!(ids, ["plain", "both", "turned", "own", "none"], "a style is not an element");
    let read = |index: usize| {
        let text = &elements[index].text;
        (text.size, text.letter_spacing, text.color, text.weight)
    };
    assert_eq!(read(0), (27.0, 2.5, [0x69, 0x6b, 0x6d, 255], 400.0));
    // A style named later wins over one named before it.
    assert_eq!(read(1), (27.0, 2.5, [255, 255, 255, 255], 700.0));
    assert_eq!(read(2), (27.0, 2.5, [0x69, 0x6b, 0x6d, 255], 700.0));
    // What the element writes itself wins over every style.
    assert_eq!(read(3), (40.0, 2.5, [0xff, 0x40, 0x40, 255], 700.0));
    assert_eq!(read(4).0, poc::model::TextStyle::default().size);
}

#[test]
fn what_is_wrong_with_a_style_is_reported_with_its_line() {
    let source = r##"<composition width="640" height="360" fps="30" duration="1">
  <style width="10"/>
  <style id="plate" width="16" height="16"/>
  <style id="plate" width="20"/>
  <style id="words" class="plate" object-fit="cover"><rect/></style>
  <rect class="plate missing" left="0" top="0"/>
  <text class="words" left="0" top="0">WORDS</text>
  <box left="0" top="0" width="16" height="16">
    <style id="late" width="4"/>
  </box>
</composition>"##;
    let problems = parse(source).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "<style> needs the attribute `id`: the name elements take it by"),
            (4, "two <style> elements have the id `plate`"),
            (5, "the style `words` has `class`; a style cannot take another style"),
            (5, "the style `words` holds an element; a style holds attributes only"),
            (6, "`class` names the style `missing`, and this file has no <style> with that id"),
            (7, "<text> has no attribute `object-fit`"),
            (9, "a <style> stands directly in the <composition>, not inside an element"),
        ]
    );
}

#[test]
fn a_style_belongs_to_its_file() {
    let folder = fixtures();
    let composition = xml::parse_file(&folder.join("host.xml"), &folder).expect("the project is valid");
    let elements = &composition.root.children;
    let (own, inner) = (&elements[0], &elements[1].children[0]);
    assert_eq!((own.id.as_str(), inner.id.as_str()), ("own", "inner"));
    // Each file's elements take the `plate` of their own file.
    let read = |node: &Node| (node.geometry.width, node.look.background[0].paint.clone());
    assert_eq!(read(own), (Some(Dim::Px(16.0)), Paint::Solid([255, 0, 0, 255])));
    assert_eq!(read(inner), (Some(Dim::Px(8.0)), Paint::Solid([0, 0, 255, 255])));

    let problems = xml::parse_file(&folder.join("faults-host.xml"), &folder)
        .err()
        .expect("the shown file names a style it does not have");
    let found: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(
        found,
        ["scenes/faults.xml, line 2: `class` names the style `plate`, and this file has no <style> with that id"]
    );
}
