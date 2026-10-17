//! What a composition file shows, read without judging it: its frame rate, with 30 when it writes none, and each
//! `<composition src>` that stands where the notation lets one stand, with its line (`docs/spec/notation.md`
//! section 10).

use notation::shows;

const ROOT: &str = "<composition version=\"1\" width=\"96\" height=\"54\"";

#[test]
fn a_file_that_writes_no_frame_rate_is_read_at_thirty() {
    let text = format!("{ROOT} duration=\"2\"/>");
    assert_eq!(shows(text.as_bytes()).fps, Some(30));
}

#[test]
fn a_written_frame_rate_is_read() {
    let text = format!("{ROOT} fps=\"24\" duration=\"2\"/>");
    assert_eq!(shows(text.as_bytes()).fps, Some(24));
}

#[test]
fn a_frame_rate_that_is_not_a_frame_rate_is_read_as_none() {
    let text = format!("{ROOT} fps=\"soon\" duration=\"2\"/>");
    assert_eq!(shows(text.as_bytes()).fps, None);
}

#[test]
fn each_element_that_shows_a_file_is_listed_with_its_line_in_document_order() {
    let text = format!(
        "{ROOT} fps=\"30\" duration=\"2\">\n\
         <composition src=\"first.xml\" left=\"0\" top=\"0\"/>\n\
         <box left=\"0\" top=\"0\" width=\"4\" height=\"4\">\n\
         <composition src=\"second.xml\" left=\"1\" top=\"1\"/>\n\
         </box>\n\
         </composition>\n"
    );
    let found = shows(text.as_bytes());
    let listed: Vec<(u32, &str)> = found.elements.iter().map(|show| (show.line, show.src.as_str())).collect();
    assert_eq!(listed, vec![(2, "first.xml"), (4, "second.xml")]);
}

#[test]
fn an_element_with_no_src_is_not_listed() {
    let text = format!("{ROOT} fps=\"30\" duration=\"2\">\n<composition left=\"0\" top=\"0\"/>\n</composition>\n");
    assert!(shows(text.as_bytes()).elements.is_empty());
}

#[test]
fn an_element_inside_a_shown_element_is_not_listed_by_the_outer_file() {
    let text = format!(
        "{ROOT} fps=\"30\" duration=\"2\">\n\
         <composition src=\"outer.xml\" left=\"0\" top=\"0\">\n\
         <composition src=\"hidden.xml\" left=\"0\" top=\"0\"/>\n\
         </composition>\n\
         </composition>\n"
    );
    let listed: Vec<String> = shows(text.as_bytes()).elements.into_iter().map(|show| show.src).collect();
    assert_eq!(listed, vec!["outer.xml".to_string()]);
}

#[test]
fn text_that_is_not_xml_shows_nothing_and_has_no_frame_rate() {
    let found = shows(b"<composition version=\"1\" fps=\"30\"");
    assert_eq!(found.fps, None);
    assert!(found.elements.is_empty());
}

#[test]
fn a_root_that_is_not_a_composition_shows_nothing() {
    let found = shows(b"<scene><composition src=\"other.xml\"/></scene>");
    assert_eq!(found.fps, None);
    assert!(found.elements.is_empty());
}
