//! A family that no font declares, a character that the font lacks, and a collection that holds no font of the
//! machine are not drawn from the machine's fonts (`docs/spec/text.md`, sections 2.3 and 2.4).

mod common;

use common::{fonts, style};
use text::{layout, Fonts, TextError};

#[test]
fn a_family_no_font_declares_is_an_error_that_names_it() {
    let mut fonts = fonts();
    let mut named = style(40.0);
    named.families = vec!["Nowhere".to_string()];
    match layout(&mut fonts, "AB", &named, None) {
        Err(TextError::UnknownFamily(name)) => assert_eq!(name, "Nowhere"),
        other => panic!("expected an unknown family, got {other:?}"),
    }
}

#[test]
fn a_character_the_font_lacks_is_an_error_that_names_it() {
    let mut fonts = fonts();
    match layout(&mut fonts, "A\u{00c9}B", &style(40.0), None) {
        Err(TextError::MissingCharacter(character)) => assert_eq!(character, '\u{00c9}'),
        other => panic!("expected a missing character, got {other:?}"),
    }
}

#[test]
fn a_new_collection_holds_no_font_of_the_machine() {
    let fonts = Fonts::new();
    assert!(fonts.is_empty(), "a new collection holds no font");
    for family in ["sans-serif", "serif", "monospace", "Helvetica"] {
        assert!(!fonts.has_family(family), "{family} is not declared by a font of the composition");
    }
}
