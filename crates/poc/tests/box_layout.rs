//! Boxes laid out in a row that is spread out, on a shared baseline, in columns, and held to a
//! widest width. See `docs/spec/native-notation.md`, section 13.
//! The fonts are this machine's, so texts are compared with each other and never with fixed
//! numbers.

use poc::model::{Composition, Node};
use poc::text::Shaper;
use poc::{layout, xml};
use std::path::Path;

fn load(source: &str) -> Composition {
    let mut composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[]);
    composition
}

/// The first element of the composition: the box under test.
fn first(composition: &Composition) -> &Node {
    &composition.root.children[0]
}

fn lefts(node: &Node) -> Vec<f64> {
    node.children.iter().map(|child| child.rect.x0).collect()
}

fn tops(node: &Node) -> Vec<f64> {
    node.children.iter().map(|child| child.rect.y0).collect()
}

fn widths(node: &Node) -> Vec<f64> {
    node.children.iter().map(|child| child.rect.width()).collect()
}

#[test]
fn a_row_spread_out_puts_the_room_left_over_between_its_items() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" width="400" height="40" flex-direction="row" justify-content="space-between" gap="10">
    <box width="50" height="20"/><box width="50" height="20"/><box width="50" height="20"/>
  </box>
</composition>"##,
    );
    // 400 less three items and two gaps leaves 230: 115 more between each two.
    assert_eq!(lefts(first(&composition)), [0.0, 175.0, 350.0]);
}

#[test]
fn a_column_spread_out_does_the_same_downwards() {
    let composition = load(
        r##"<composition width="800" height="400" fps="10" duration="1">
  <box left="0" top="0" width="100" height="300" flex-direction="column" justify-content="space-between">
    <box width="50" height="20"/><box width="50" height="20"/><box width="50" height="20"/>
  </box>
</composition>"##,
    );
    assert_eq!(tops(first(&composition)), [0.0, 140.0, 280.0]);
}

#[test]
fn one_item_or_no_room_left_over_is_spread_as_from_the_start() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" width="400" height="40" flex-direction="row" justify-content="space-between">
    <box width="50" height="20"/>
  </box>
  <box left="0" top="50" width="100" height="40" flex-direction="row" justify-content="space-between" gap="10">
    <box width="80" height="20"/><box width="80" height="20"/>
  </box>
</composition>"##,
    );
    assert_eq!(lefts(&composition.root.children[0]), [0.0]);
    assert_eq!(lefts(&composition.root.children[1]), [0.0, 90.0]);
}

#[test]
fn items_on_a_baseline_have_their_first_lines_on_one_line() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" flex-direction="row" align-items="baseline" gap="10">
    <text font-size="80">99</text>
    <text font-size="20">percent</text>
    <box width="30" height="12"/>
  </box>
</composition>"##,
    );
    let row = first(&composition);
    let (large, small, plain) = (&row.children[0], &row.children[1], &row.children[2]);
    let line = large.rect.y0 + large.baseline;
    assert!(
        (small.rect.y0 + small.baseline - line).abs() < 1e-6,
        "the two texts stand on one line"
    );
    // What holds no line stands on it with its bottom edge.
    assert!((plain.rect.y1 - line).abs() < 1e-6);
    // The highest touches the top, and the row is as high as its items need.
    assert_eq!(large.rect.y0, 0.0);
    assert!(small.rect.y0 > 0.0);
    let lowest = row.children.iter().map(|child| child.rect.y1).fold(0.0, f64::max);
    assert!((row.rect.height() - lowest).abs() < 1e-6);
}

#[test]
fn an_item_can_ask_for_the_baseline_itself_and_a_row_has_that_of_its_first_item() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" height="150" flex-direction="row" align-items="flex-start">
    <text font-size="80" align-self="baseline">99</text>
    <box flex-direction="row" padding="7 0 0 0" align-self="baseline"><text font-size="20">percent</text></box>
    <text font-size="20">top</text>
  </box>
</composition>"##,
    );
    let row = first(&composition);
    let (large, inner, top) = (&row.children[0], &row.children[1], &row.children[2]);
    let word = &inner.children[0];
    let line = large.rect.y0 + large.baseline;
    assert!((inner.rect.y0 + word.rect.y0 + word.baseline - line).abs() < 1e-6);
    assert_eq!(top.rect.y0, 0.0);
}

#[test]
fn in_a_column_the_baseline_is_the_start() {
    let composition = load(
        r##"<composition width="800" height="200" fps="10" duration="1">
  <box left="0" top="0" width="300" flex-direction="column" align-items="baseline">
    <box width="50" height="20"/><box width="80" height="20"/>
  </box>
</composition>"##,
    );
    assert_eq!(lefts(first(&composition)), [0.0, 0.0]);
    assert_eq!(tops(first(&composition)), [0.0, 20.0]);
}

#[test]
fn columns_of_pixels_and_shares_fill_row_by_row() {
    let composition = load(
        r##"<composition width="800" height="400" fps="10" duration="1">
  <box left="0" top="0" width="500" columns="100 1fr 3fr" gap="20">
    <box height="30"/><box height="50"/><box/>
    <box height="10"/><box width="40" height="10"/>
  </box>
</composition>"##,
    );
    let grid = first(&composition);
    // 500 less 100 and two gaps leaves 360: 90 and 270.
    assert_eq!(lefts(grid), [0.0, 120.0, 230.0, 0.0, 120.0]);
    // An item is as wide as its column, unless it has a width of its own.
    assert_eq!(widths(grid), [100.0, 90.0, 270.0, 100.0, 40.0]);
    // A row is as high as its tallest item, and an item with no height is as high as its row.
    assert_eq!(tops(grid), [0.0, 0.0, 0.0, 70.0, 70.0]);
    assert_eq!(grid.children[2].rect.height(), 50.0);
    assert_eq!(grid.rect.height(), 80.0);
}

#[test]
fn rows_share_the_height_left_over_and_items_stand_in_them_as_asked() {
    let composition = load(
        r##"<composition width="800" height="400" fps="10" duration="1">
  <box left="0" top="0" width="200" height="200" columns="1fr" gap="20" align-items="center" padding="0 0 0 10">
    <box height="50"/><box height="30" align-self="flex-end"/>
  </box>
</composition>"##,
    );
    let grid = first(&composition);
    // 200 less 80 and a gap leaves 100: the rows are 100 and 80 high.
    assert_eq!(tops(grid), [25.0, 120.0 + 50.0]);
    assert_eq!(lefts(grid), [10.0, 10.0]);
    assert_eq!(widths(grid), [190.0, 190.0]);
}

#[test]
fn a_share_column_of_a_box_with_no_width_is_as_wide_as_its_widest_item() {
    let composition = load(
        r##"<composition width="800" height="400" fps="10" duration="1">
  <box left="0" top="0" columns="60 1fr" gap="5">
    <box height="10"/><box width="70" height="10"/>
    <box height="10"/><box width="120" height="10"/>
  </box>
</composition>"##,
    );
    assert_eq!(first(&composition).rect.width(), 60.0 + 5.0 + 120.0);
}

#[test]
fn a_child_placed_by_its_edges_is_no_item_of_the_columns() {
    let composition = load(
        r##"<composition width="800" height="400" fps="10" duration="1">
  <box left="0" top="0" width="200" columns="1fr 1fr">
    <box height="10"/><box position="absolute" left="7" top="9" width="3" height="3"/><box height="10"/>
  </box>
</composition>"##,
    );
    assert_eq!(lefts(first(&composition)), [0.0, 7.0, 100.0]);
    assert_eq!(tops(first(&composition)), [0.0, 9.0, 0.0]);
}

#[test]
fn a_widest_width_holds_what_is_given_and_breaks_a_text_into_lines() {
    let words = "Infrastructure that stays out of the way, so the work can stay in front.";
    let composition = load(&format!(
        r##"<composition width="1920" height="400" fps="10" duration="1">
  <box left="0" top="0" width="900" flex-direction="column">
    <text font-size="20">{words}</text>
    <text font-size="20" max-width="300">{words}</text>
    <box height="10" max-width="120"/>
    <box height="10" width="200" max-width="120"/>
  </box>
  <text left="0" top="200" font-size="20" max-width="300">{words}</text>
</composition>"##
    ));
    let column = first(&composition);
    let (free, held) = (&column.children[0], &column.children[1]);
    assert_eq!(free.rect.width(), 900.0);
    assert_eq!(held.rect.width(), 300.0);
    assert!(held.rect.height() > free.rect.height(), "the held text has more lines");
    assert_eq!(column.children[2].rect.width(), 120.0);
    // A width of its own is not held.
    assert_eq!(column.children[3].rect.width(), 200.0);
    // A text with no width at all breaks at its widest width and is as wide as its lines.
    let alone = &composition.root.children[1];
    assert!(alone.rect.width() <= 300.0 && alone.rect.width() > 150.0);
    assert!((alone.rect.height() - held.rect.height()).abs() < 1e-6);
}

#[test]
fn what_a_layout_may_not_be_is_refused_with_its_line() {
    let source = r##"<composition width="320" height="64" fps="10" duration="1">
  <box columns="1fr 2fr" flex-direction="row"/>
  <box columns="wide 1fr"/>
  <box columns="0 1fr"/>
  <text columns="1fr">words</text>
  <box flex-direction="row" justify-content="baseline"/>
  <box flex-direction="row" align-items="space-between"/>
  <box max-width="wide"/>
</composition>"##;
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "a <box> has `columns` or `flex-direction`, not both"),
            (
                3,
                "`columns=\"wide 1fr\"` is not a list of column widths: pixels, or shares such as `1fr`"
            ),
            (
                4,
                "`columns=\"0 1fr\"` is not a list of column widths: pixels, or shares such as `1fr`"
            ),
            (5, "`columns` is for a <box>"),
            (
                6,
                "`justify-content=\"baseline\"` is not one of: flex-start, center, flex-end, space-between"
            ),
            (
                7,
                "`align-items=\"space-between\"` is not one of: flex-start, center, flex-end, stretch, baseline"
            ),
            (8, "`max-width=\"wide\"` is not a number"),
        ]
    );
}
