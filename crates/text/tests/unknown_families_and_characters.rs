//! The family list of `font-family` and the fall-back of a character the listed families lack (`docs/spec/text.md`,
//! sections 2.3, 2.4 and 2.7): an absent name is passed over, a present first name draws, a character the test font lacks
//! is drawn from the first family of the machine in the stated order that maps it, and a character no font maps is drawn
//! as the missing-character mark and named by the block. The tests that read the machine's fonts are compiled for macOS
//! only, since the machine's fonts are read there only (decision A9).

mod common;

use common::{fonts, style, FAMILY};
use text::{layout, Block, Fonts};

/// The glyph ids of a block, with the bytes of the font of each run, in the order of the runs.
fn drawn(block: &Block) -> Vec<(u32, usize)> {
    block
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |glyph| (glyph.id, run.font.len())))
        .collect()
}

fn in_families(names: &[&str]) -> text::Style {
    let mut named = style(40.0);
    named.families = names.iter().map(|name| name.to_string()).collect();
    named
}

#[test]
fn a_new_collection_lists_no_family_and_resolves_no_family_of_the_machine() {
    // The engine's own collection, asked directly: it lists no family, and a family that every Mac has, or a generic
    // name, resolves to nothing in it. The machine's fonts join it only through section 2.7.
    let mut fonts = Fonts::new(&codec::Registry::new());
    assert_eq!(fonts.engine_family_names(), Vec::<String>::new(), "a new collection lists a family");
    for family in ["Helvetica", "Times New Roman", "Menlo", "sans-serif", "serif", "monospace"] {
        assert!(!fonts.engine_resolves(family), "{family} resolves in a new collection");
    }
}

#[test]
fn a_text_whose_first_family_is_absent_is_drawn_with_the_second() {
    let mut with_absent = fonts();
    let passed_over = layout(&mut with_absent, "AB", &in_families(&["Nowhere At All", FAMILY]), None).unwrap();
    let mut plain = fonts();
    let alone = layout(&mut plain, "AB", &style(40.0), None).unwrap();
    assert_eq!(drawn(&passed_over), drawn(&alone));
    assert_eq!(passed_over.width, alone.width);
    assert!(
        with_absent.machine_faces().is_empty(),
        "no face of the machine draws: {:?}",
        with_absent.machine_faces()
    );
}

#[test]
fn a_family_whose_font_is_not_registered_is_absent_and_passed_over() {
    // A family a <font> would have declared, had its file been taken, is absent like any other unknown name.
    let mut fonts = fonts();
    let block = layout(&mut fonts, "AB", &in_families(&["Damaged Family", FAMILY]), None).unwrap();
    assert_eq!(block.missing, None);
    assert_eq!(block.runs.len(), 1);
}

#[cfg(target_os = "macos")]
mod on_the_machine {
    use super::*;

    const HELVETICA: &str = "/System/Library/Fonts/Helvetica.ttc";

    /// The machine face of each run of a block, by its file and index, or `None` for a face of the composition.
    fn faces(fonts: &Fonts, block: &Block) -> Vec<Option<(String, u32, String)>> {
        block
            .runs
            .iter()
            .map(|run| {
                fonts
                    .machine_face_of(&run.font)
                    .map(|face| (face.path.display().to_string(), face.index, face.family))
            })
            .collect()
    }

    #[test]
    fn a_text_whose_first_family_is_present_is_drawn_with_the_first() {
        let mut fonts = fonts();
        let block = layout(&mut fonts, "AB", &in_families(&[FAMILY, "Helvetica"]), None).unwrap();
        assert_eq!(faces(&fonts, &block), vec![None], "the test font draws every letter");
    }

    #[test]
    fn a_family_of_the_machine_is_matched_without_regard_to_case_and_draws() {
        let mut fonts = Fonts::new(&codec::Registry::new());
        let block = layout(&mut fonts, "AB", &in_families(&["helvetica"]), None).unwrap();
        let drawn = faces(&fonts, &block);
        println!("`helvetica` drew with {drawn:?}");
        assert_eq!(drawn.len(), 1);
        let (path, _, family) = drawn[0].clone().expect("a face of the machine draws");
        assert_eq!(path, HELVETICA);
        assert_eq!(family, "Helvetica");
    }

    #[test]
    fn the_generic_names_stand_for_helvetica_times_and_menlo() {
        for (generic, file) in [
            ("sans-serif", "/System/Library/Fonts/Helvetica.ttc"),
            ("serif", "/System/Library/Fonts/Times.ttc"),
            ("monospace", "/System/Library/Fonts/Menlo.ttc"),
        ] {
            let mut fonts = Fonts::new(&codec::Registry::new());
            let block = layout(&mut fonts, "AB", &in_families(&[generic]), None).unwrap();
            let drawn = faces(&fonts, &block);
            println!("`{generic}` drew with {drawn:?}");
            assert_eq!(drawn.len(), 1, "{generic}");
            assert_eq!(drawn[0].as_ref().map(|face| face.0.as_str()), Some(file), "{generic}");
        }
    }

    #[test]
    fn a_character_the_test_font_lacks_is_drawn_from_helvetica_and_the_rest_from_the_test_font() {
        // The test font maps U+0020 to U+007E only (`docs/spec/test-font.md`, section 2): it lacks the É.
        let mut fonts = fonts();
        let block = layout(&mut fonts, "A\u{00c9}B", &style(40.0), None).unwrap();
        let drawn = faces(&fonts, &block);
        println!("A\u{00c9}B drew with {drawn:?}");
        assert_eq!(drawn.len(), 3, "three runs: the test font, the machine's face, the test font");
        assert_eq!(drawn[0], None);
        assert_eq!(drawn[2], None);
        let (path, _, family) = drawn[1].clone().expect("the É is drawn by a face of the machine");
        assert_eq!((path.as_str(), family.as_str()), (HELVETICA, "Helvetica"));
        assert_eq!(block.missing, None);
    }

    #[test]
    fn the_fall_back_takes_the_first_family_in_its_stated_order_that_maps_the_character() {
        // Helvetica has no Thai letter and Arial Unicode MS has, so the Thai letter goes to the second of the order. The emoji
        // half of this case, which needs the system's codec backend, is in the render crate's tests.
        let mut fonts = fonts();
        let thai = layout(&mut fonts, "A\u{0e01}", &style(40.0), None).unwrap();
        let drawn = faces(&fonts, &thai);
        println!("A\u{0e01} drew with {drawn:?}");
        assert_eq!(drawn[0], None);
        assert_eq!(drawn[1].as_ref().map(|face| face.2.as_str()), Some("Arial Unicode MS"));
    }

    #[test]
    fn a_character_no_font_maps_is_drawn_as_the_missing_character_mark_of_the_first_family() {
        // U+0378 is not assigned in Unicode: no font of the machine maps it.
        let mut fonts = fonts();
        let block = layout(&mut fonts, "A\u{0378}B", &style(40.0), None).unwrap();
        assert_eq!(block.missing, Some('\u{0378}'));
        assert_eq!(
            faces(&fonts, &block),
            vec![None],
            "the test font draws all three, the mark among them"
        );
        let ids: Vec<u32> = block.runs[0].glyphs.iter().map(|glyph| glyph.id).collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[1], 0, "the mark is glyph 0");
        assert!(fonts.machine_faces().is_empty());
    }

    #[test]
    fn a_text_whose_names_are_all_absent_is_drawn_with_helvetica() {
        let mut fonts = Fonts::new(&codec::Registry::new());
        let block = layout(&mut fonts, "AB", &in_families(&["Nowhere At All"]), None).unwrap();
        let drawn = faces(&fonts, &block);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].as_ref().map(|face| face.0.as_str()), Some(HELVETICA));
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_text_whose_names_are_all_absent_has_no_font_where_the_machine_fonts_are_not_read() {
    let mut fonts = fonts();
    assert_eq!(
        layout(&mut fonts, "AB", &in_families(&["Nowhere At All"]), None),
        Err(text::TextError::NoFamily)
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn a_character_the_test_font_lacks_is_drawn_as_the_missing_mark_where_the_machine_fonts_are_not_read() {
    let mut fonts = fonts();
    let block = layout(&mut fonts, "A\u{00c9}B", &style(40.0), None).unwrap();
    assert_eq!(block.missing, Some('\u{00c9}'));
}
