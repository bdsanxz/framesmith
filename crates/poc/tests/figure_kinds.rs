//! The kind of figures a text uses. See `docs/spec/native-notation.md`, section 14.
//! The font is this machine's own (`system-ui`), whose figures are of their own widths unless
//! asked otherwise; widths are compared with each other and never with fixed numbers.

use poc::model::{Composition, Node};
use poc::text::Shaper;
use poc::{layout, xml};
use std::path::Path;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

fn first(composition: &Composition) -> &Node {
    &composition.root.children[0]
}

/// The features a text asks its font for, as the shaper is given them.
fn figures(node: &Node) -> &str {
    &node.text.figures
}

#[test]
fn the_kinds_are_read_as_the_features_of_the_font_and_inherited() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" font-variant-numeric="tabular-nums lining-nums">
    <text>1</text>
    <box font-variant-numeric="oldstyle-nums proportional-nums">
      <text>2</text>
      <text font-variant-numeric="normal">3</text>
    </box>
  </box>
  <text left="0" top="100">4</text>
</composition>"##,
    );
    let outer = &composition.root.children[0];
    assert_eq!(figures(&outer.children[0]), "\"tnum\" 1, \"lnum\" 1");
    assert_eq!(figures(&outer.children[1].children[0]), "\"onum\" 1, \"pnum\" 1");
    assert_eq!(figures(&outer.children[1].children[1]), "");
    assert_eq!(figures(&composition.root.children[1]), "");
}

#[test]
fn tabular_figures_are_all_of_one_width() {
    let composition = load(
        r##"<composition width="1920" height="400" fps="10" duration="1">
  <box left="0" top="0" right="0" bottom="0" font-family="system-ui" font-size="100">
    <text position="absolute" left="0" top="0">1111</text>
    <text position="absolute" left="0" top="100">0000</text>
    <text position="absolute" left="0" top="200" font-variant-numeric="tabular-nums">1111</text>
    <text position="absolute" left="0" top="300" font-variant-numeric="tabular-nums">0000</text>
  </box>
</composition>"##,
    );
    let widths: Vec<f64> = first(&composition).children.iter().map(|text| text.rect.width()).collect();
    assert!(
        widths[0] < widths[1] - 10.0,
        "the font's own ones are narrower than its noughts: {widths:?}"
    );
    assert!((widths[2] - widths[3]).abs() < 1e-6, "{widths:?}");
    assert!(
        widths[2] > widths[0] + 10.0,
        "a tabular one is wider than the font's own: {widths:?}"
    );
}

#[test]
fn a_kind_that_is_not_known_is_refused_with_its_line() {
    let source = r##"<composition width="320" height="64" fps="10" duration="1">
  <text font-variant-numeric="slashed-zero">0</text>
  <text font-variant-numeric="">0</text>
  <text font-variant-numeric="normal tabular-nums">0</text>
</composition>"##;
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    let form = "is not `normal` or a list of: lining-nums, oldstyle-nums, tabular-nums, proportional-nums";
    assert_eq!(
        found,
        [
            (2, format!("`font-variant-numeric=\"slashed-zero\"` {form}").as_str()),
            (3, format!("`font-variant-numeric=\"\"` {form}").as_str()),
            (4, format!("`font-variant-numeric=\"normal tabular-nums\"` {form}").as_str()),
        ]
    );
}
