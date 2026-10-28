//! The program's support table is the table of `docs/spec/notation.md` section 14: the same entries, each with the
//! reason and the sprint the spec words. The spec is read here as text, so an entry added, struck or reworded on one
//! side and not the other fails a test that names the entry.

use notation::Unsupported;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The rows of section 14 of the spec: for each entry's number, its reason and its sprint, as written.
fn spec_rows() -> BTreeMap<u32, (String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/notation.md");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let section = text
        .split("## 14. The support table")
        .nth(1)
        .and_then(|rest| rest.split("\n## 15.").next())
        .expect("the spec has a section 14 followed by a section 15");
    let mut rows = BTreeMap::new();
    for line in section.lines() {
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        let Some(number) = cells
            .first()
            .and_then(|cell| cell.strip_prefix('S'))
            .and_then(|digits| digits.parse::<u32>().ok())
        else {
            continue;
        };
        assert_eq!(cells.len(), 5, "S{number:02}: a row of the table has five cells");
        let previous = rows.insert(number, (cells[3].to_string(), cells[4].to_string()));
        assert!(previous.is_none(), "S{number:02} is written twice in the spec");
    }
    rows
}

#[test]
fn every_entry_of_the_spec_is_an_entry_of_the_program_with_its_reason_and_its_sprint() {
    let rows = spec_rows();
    assert!(rows.len() > 60, "section 14 was read: {} rows", rows.len());
    for (number, (reason, waits)) in &rows {
        let entry = Unsupported::ALL
            .into_iter()
            .find(|entry| entry.number() == *number)
            .unwrap_or_else(|| panic!("S{number:02} of the spec is not an entry of the program"));
        assert_eq!(entry.reason(), reason, "S{number:02}: the reason");
        assert_eq!(entry.waits(), waits, "S{number:02}: the sprint");
    }
}

#[test]
fn every_entry_of_the_program_is_on_the_spec_table() {
    let rows = spec_rows();
    for entry in Unsupported::ALL {
        assert!(
            rows.contains_key(&entry.number()),
            "{entry} is an entry of the program but not of the spec"
        );
    }
}

#[test]
fn an_entry_is_named_by_its_number_with_two_digits() {
    // Worked from the spec's numbering: S01 is the first entry, S34 the film's kind of file.
    let first = Unsupported::ALL[0];
    assert_eq!(first.name(), "S01");
    assert_eq!(Unsupported::FilmNotMp4.name(), "S34");
}

#[test]
fn the_message_names_what_was_written_the_entry_its_reason_and_its_sprint() {
    let message = Unsupported::InnerShadow.message("<box> has `box-shadow=\"inset 0 0 4 #000000\"`");
    assert_eq!(
        message,
        "<box> has `box-shadow=\"inset 0 0 4 #000000\"`, which this version does not support (support table S73: Only shadows outside the box are drawn (ROADMAP 5.13); waits until Sprint 5)"
    );
}
