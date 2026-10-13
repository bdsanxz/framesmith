//! A picture fitted to its box, and where in the box it is held. See
//! `docs/spec/native-notation.md`, section 15.

use poc::cpu::Cpu;
use poc::decode::Picture;
use poc::model::{Composition, Fit, FitKind, Kind};
use poc::paint;
use poc::scene::Evaluator;
use poc::text::Shaper;
use poc::{layout, xml, Backend};
use std::path::Path;
use vello_cpu::kurbo::{Point, Rect};

const WIDTH: usize = 200;
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREY: [u8; 3] = [128, 128, 128];

/// A picture of 40 by 20: its left half red, its right half blue.
fn halves() -> Picture {
    let mut pixels = Vec::new();
    for _ in 0..20 {
        for x in 0..40 {
            pixels.extend_from_slice(if x < 20 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    Picture {
        width: 40,
        height: 20,
        pixels,
        opaque: true,
    }
}

/// A grey frame of 200 by 100 with the picture in a box of 100 by 100 at its left, and the
/// frame's pixels.
fn drawn(attributes: &str) -> Vec<u8> {
    let source = format!(
        r##"<composition width="200" height="100" fps="10" duration="1" background="#808080">
  <image src="halves.png" left="0" top="0" width="100" height="100" {attributes}/>
</composition>"##
    );
    let mut composition: Composition = xml::parse(&source, Path::new(".")).expect("the scene is valid");
    layout::lay_out(&mut composition, &mut Shaper::new(), &[(40, 20)]);
    let mut cpu = Cpu::new(&composition, &[halves()]);
    cpu.draw(0, &Evaluator::new(&composition).frame(0).ops).expect("the frame is drawn");
    cpu.pixels().expect("the frame is read").to_vec()
}

fn at(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    let place = (y * WIDTH + x) * 4;
    [pixels[place], pixels[place + 1], pixels[place + 2]]
}

#[test]
fn a_picture_that_covers_is_held_where_asked() {
    let hold = |across: f64, down: f64| Fit {
        kind: FitKind::Cover,
        hold: (across, down),
    };
    let rect = Rect::new(10.0, 20.0, 110.0, 120.0);
    // 40 by 20 covers 100 by 100 at five times its size: 200 by 100, 100 too wide.
    let (placed, drawn_in) = paint::fit(rect, 40.0, 20.0, hold(0.5, 0.5));
    assert_eq!(placed * Point::new(0.0, 0.0), Point::new(-40.0, 20.0));
    assert_eq!(placed * Point::new(40.0, 20.0), Point::new(160.0, 120.0));
    assert_eq!(drawn_in, rect);
    assert_eq!(
        paint::fit(rect, 40.0, 20.0, hold(0.0, 0.5)).0 * Point::new(0.0, 0.0),
        Point::new(10.0, 20.0)
    );
    assert_eq!(
        paint::fit(rect, 40.0, 20.0, hold(1.0, 0.5)).0 * Point::new(0.0, 0.0),
        Point::new(-90.0, 20.0)
    );
    // A picture taller than its box is held up and down.
    assert_eq!(
        paint::fit(rect, 20.0, 40.0, hold(0.5, 0.0)).0 * Point::new(0.0, 0.0),
        Point::new(10.0, 20.0)
    );
    assert_eq!(
        paint::fit(rect, 20.0, 40.0, hold(0.5, 0.25)).0 * Point::new(0.0, 0.0),
        Point::new(10.0, -5.0)
    );
}

#[test]
fn a_picture_fitted_whole_keeps_its_shape_and_is_drawn_in_its_own_place_only() {
    let whole = |across: f64, down: f64| Fit {
        kind: FitKind::Contain,
        hold: (across, down),
    };
    let rect = Rect::new(10.0, 20.0, 110.0, 120.0);
    // 40 by 20 fits 100 by 100 at two and a half times its size: 100 by 50.
    let (placed, drawn_in) = paint::fit(rect, 40.0, 20.0, whole(0.5, 0.5));
    assert_eq!(placed * Point::new(0.0, 0.0), Point::new(10.0, 45.0));
    assert_eq!(placed * Point::new(40.0, 20.0), Point::new(110.0, 95.0));
    assert_eq!(drawn_in, Rect::new(10.0, 45.0, 110.0, 95.0));
    assert_eq!(paint::fit(rect, 40.0, 20.0, whole(0.5, 0.0)).1, Rect::new(10.0, 20.0, 110.0, 70.0));
    assert_eq!(paint::fit(rect, 40.0, 20.0, whole(0.5, 1.0)).1, Rect::new(10.0, 70.0, 110.0, 120.0));
    // A picture stretched to its box fills it, whatever its shape.
    let (placed, drawn_in) = paint::fit(rect, 40.0, 20.0, Fit::FILL);
    assert_eq!(placed * Point::new(40.0, 20.0), Point::new(110.0, 120.0));
    assert_eq!(drawn_in, rect);
}

#[test]
fn the_pixels_of_a_covering_picture_are_those_of_the_part_held() {
    // Five times its size: 200 wide in a box of 100. Held at its middle, the halves meet in
    // the middle of the box.
    let middle = drawn(r#"object-fit="cover""#);
    assert_eq!(at(&middle, 40, 50), RED);
    assert_eq!(at(&middle, 60, 50), BLUE);
    assert_eq!(at(&middle, 96, 5), BLUE);
    // The box cuts the picture: beside it the frame shows.
    assert_eq!(at(&middle, 104, 50), GREY);
    let left = drawn(r#"object-fit="cover" object-position="0% 50%""#);
    assert_eq!(at(&left, 4, 50), RED);
    assert_eq!(at(&left, 94, 95), RED);
    let right = drawn(r#"object-fit="cover" object-position="100% 50%""#);
    assert_eq!(at(&right, 4, 50), BLUE);
    assert_eq!(at(&right, 94, 95), BLUE);
    assert_eq!(at(&right, 104, 50), GREY);
}

#[test]
fn the_pixels_of_a_picture_fitted_whole_leave_the_rest_of_the_box_open() {
    // Two and a half times its size: 100 by 50, in the middle of the box.
    let middle = drawn(r#"object-fit="contain""#);
    assert_eq!(at(&middle, 20, 50), RED);
    assert_eq!(at(&middle, 80, 50), BLUE);
    assert_eq!(at(&middle, 20, 28), RED);
    assert_eq!(at(&middle, 80, 72), BLUE);
    assert_eq!(at(&middle, 20, 20), GREY);
    assert_eq!(at(&middle, 80, 80), GREY);
    let top = drawn(r#"object-fit="contain" object-position="50% 0%""#);
    assert_eq!(at(&top, 20, 3), RED);
    assert_eq!(at(&top, 80, 46), BLUE);
    assert_eq!(at(&top, 80, 54), GREY);
    // Stretched, as when nothing is written, both halves reach from top to bottom.
    let stretched = drawn("");
    assert_eq!(at(&stretched, 20, 3), RED);
    assert_eq!(at(&stretched, 80, 96), BLUE);
}

#[test]
fn the_fit_and_the_place_are_read_and_what_they_may_not_be_is_refused() {
    let source = r##"<composition width="320" height="64" fps="10" duration="1">
  <image src="a.png" object-fit="contain" object-position="25% 100%"/>
  <image src="a.png"/>
</composition>"##;
    let composition = xml::parse(source, Path::new(".")).expect("the scene is valid");
    let fits: Vec<Fit> = composition
        .root
        .children
        .iter()
        .map(|node| match node.kind {
            Kind::Image { fit, .. } => fit,
            _ => panic!("the element is a picture"),
        })
        .collect();
    assert_eq!(fits[0].kind, FitKind::Contain);
    assert_eq!(fits[0].hold, (0.25, 1.0));
    assert_eq!(fits[1].kind, FitKind::Fill);
    assert_eq!(fits[1].hold, (0.5, 0.5));

    let source = r##"<composition width="320" height="64" fps="10" duration="1">
  <image src="a.png" object-fit="scale-down"/>
  <image src="a.png" object-position="top"/>
  <image src="a.png" object-position="50%"/>
  <image src="a.png" object-position="10px 20px"/>
  <box object-position="50% 50%"/>
</composition>"##;
    let problems = xml::parse(source, Path::new(".")).err().expect("the file has problems");
    let found: Vec<(u32, &str)> = problems.iter().map(|p| (p.line, p.message.as_str())).collect();
    assert_eq!(
        found,
        [
            (2, "`object-fit=\"scale-down\"` is not one of: fill, cover, contain"),
            (3, "`object-position=\"top\"` is not two percentages, across and down"),
            (4, "`object-position=\"50%\"` is not two percentages, across and down"),
            (5, "`object-position=\"10px 20px\"` is not two percentages, across and down"),
            (6, "<box> has no attribute `object-position`"),
        ]
    );
}
