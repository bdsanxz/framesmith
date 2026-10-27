//! The checked-in text pictures that the stroke, fill and baseline rules changed hold, pixel by pixel, to the worked
//! pictures of their fixtures (the `worked` folders): each pixel of each frame within one level in each colour. The worked
//! pictures are the text model of `evidence/textgaps/text_model.py`, worked from the display list of each frame and the font
//! file, with the coverage rules of `docs/spec/text.md` (section 6.6, rules 5, 6 and 10) and the layers rounded as
//! `docs/spec/render-command.md` (section 4.5, rule 6) gives.

mod common;

use common::held::{drawn, within_levels_where_held, worked};
use common::numbers_with_a_picture;

const FIXTURES: [&str; 25] = [
    "declared-font-file",
    "shared-baseline-texts",
    "text-attributes-inherited",
    "text-colour-and-weight",
    "text-default-colour",
    "text-dressing-blurred-shadow",
    "text-dressing-no-wrap",
    "text-dressing-shadow",
    "text-dressing-two-shadows",
    "text-dressing-upper-case",
    "text-fill-arcs",
    "text-figure-oldstyle",
    "text-figure-widths",
    "text-line-height-and-spacing",
    "text-span-room-at-a-break",
    "text-span-room-at-a-hyphen",
    "text-span-room-at-start",
    "text-span-room-before-a-full-stop",
    "text-spans-colour-and-baseline",
    "text-spans-inherit-attributes",
    "text-spans-larger-size",
    "text-spans-line-break",
    "text-spans-margin-and-padding",
    "text-wrapping-and-alignment",
    "widest-width-text",
];

#[test]
fn every_pixel_of_each_changed_text_picture_is_within_one_level_of_its_worked_picture() {
    for fixture in FIXTURES {
        let numbers = numbers_with_a_picture(fixture);
        assert!(!numbers.is_empty(), "{fixture} has checked-in pictures");
        for number in numbers {
            let (worked, held) = worked(fixture, number);
            assert!(held.iter().all(|&h| h), "{fixture}, frame {number}: every pixel is held");
            let checked = within_levels_where_held(&drawn("text-pictures-worked", fixture, number), &worked, &held, 1)
                .unwrap_or_else(|error| panic!("{fixture}, frame {number}: {error}"));
            assert_eq!(checked, (worked.width() * worked.height()) as usize, "{fixture}, frame {number}: every pixel is checked");
        }
    }
}
